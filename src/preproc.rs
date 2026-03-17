use crate::adapted_iter::AdaptedFilesIterBox;
use crate::adapters::*;
use crate::caching_writer::async_read_and_write_to_cache;
use crate::config::RgaConfig;
use crate::matching::*;
use crate::preproc_cache::CacheKey;
use crate::recurse::concat_read_streams;
use crate::{
    preproc_cache::{PreprocCache, open_cache_db},
    print_bytes,
};
use anyhow::*;
use async_compression::tokio::bufread::ZstdDecoder;
use async_stream::stream;
// use futures::future::{BoxFuture, FutureExt};
use log::*;
use postproc::PostprocPrefix;
use std::future::Future;
use std::io::Cursor;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use tokio::io::{AsyncBufRead, AsyncReadExt};

pub type ActiveAdapters = Vec<Arc<dyn FileAdapter>>;

async fn choose_adapter(
    config: &RgaConfig,
    filepath_hint: &Path,
    archive_recursion_depth: i32,
    inp: &mut (impl AsyncBufRead + Unpin),
    active_adapters: Option<&ActiveAdapters>,
) -> Result<Option<(Arc<dyn FileAdapter>, FileMatcher, ActiveAdapters)>> {
    let computed_adapters;
    let active_adapters = match active_adapters {
        Some(a) => a,
        None => {
            computed_adapters = get_adapters_filtered(config.custom_adapters.clone(), &config.adapters, config)?;
            &computed_adapters
        }
    };
    let adapters = adapter_matcher(active_adapters, config.accurate)?;
    let filename = filepath_hint
        .file_name()
        .ok_or_else(|| format_err!("Empty filename"))?;
    debug!("Archive recursion depth: {}", archive_recursion_depth);

    let mimetype = if config.accurate {
        let buf = inp.fill_buf().await?; // fill but do not consume!
        if buf.starts_with(b"From \x0d") || buf.starts_with(b"From -") {
            Some("application/mbox")
        } else {
            let mimetype = infer::get(buf).map(|t| t.mime_type());
            debug!("mimetype: {:?}", mimetype);
            mimetype
        }
    } else {
        None
    };
    let adapter = adapters(FileMeta {
        mimetype,
        lossy_filename: filename.to_string_lossy().to_string(),
    });
    Ok(adapter.map(|e| (e.0, e.1, active_adapters.clone())))
}

enum Ret {
    Recurse(AdaptInfo, Arc<dyn FileAdapter>, FileMatcher, ActiveAdapters),
    Passthrough(AdaptInfo),
}
async fn buf_choose_adapter(ai: AdaptInfo, active_adapters: Option<&ActiveAdapters>) -> Result<Ret> {
    // Only use a buffer if we need to detect mime types (accurate mode)
    // or if it's a real file (where buffering helps performance).
    // For files already in memory or from other streams, a large buffer might be redundant.
    let capacity = if ai.config.accurate { 8192 } else { 1024 };
    let mut inp = BufReader::with_capacity(capacity, ai.inp);
    let adapter = choose_adapter(
        &ai.config,
        &ai.filepath_hint,
        ai.archive_recursion_depth,
        &mut inp,
        active_adapters,
    )
    .await?;
    let ai = AdaptInfo {
        inp: Box::pin(inp),
        ..ai
    };
    let (a, b, c) = match adapter {
        Some(x) => x,
        None => {
            // allow passthrough if the file is in an archive or accurate matching is enabled
            // otherwise it should have been filtered out by rg pre-glob since rg can handle those better than us
            let allow_cat = !ai.is_real_file || ai.config.accurate;
            if allow_cat {
                if ai.postprocess {
                    (
                        Arc::new(PostprocPrefix {}) as Arc<dyn FileAdapter>,
                        FileMatcher::Fast(FastFileMatcher::FileExtension("default".to_string())),
                        Vec::new(),
                    )
                } else {
                    return Ok(Ret::Passthrough(ai));
                }
            } else {
                return Err(format_err!(
                    "No adapter found for file {:?}, passthrough disabled.",
                    ai.filepath_hint
                        .file_name()
                        .ok_or_else(|| format_err!("Empty filename"))?
                ));
            }
        }
    };
    Ok(Ret::Recurse(ai, a, b, c))
}

/**
 * preprocess a file as defined in `ai`.
 *
 * If a cache is passed, read/write to it.
 *
 */
pub async fn rga_preproc(ai: AdaptInfo) -> Result<ReadBox> {
    debug!("path (hint) to preprocess: {:?}", ai.filepath_hint);

    // todo: figure out when using a bufreader is a good idea and when it is not
    // seems to be good for File::open() reads, but not sure about within archives (tar, zip)
    let (ai, adapter, detection_reason, active_adapters) = match buf_choose_adapter(ai, None).await? {
        Ret::Recurse(ai, a, b, c) => (ai, a, b, c),
        Ret::Passthrough(ai) => {
            return Ok(ai.inp);
        }
    };
    let path_hint_copy = ai.filepath_hint.clone();
    adapt_caching(ai, adapter, detection_reason, active_adapters)
        .await
        .with_context(|| format!("run_adapter({})", &path_hint_copy.to_string_lossy()))
}

async fn adapt_caching(
    ai: AdaptInfo,
    adapter: Arc<dyn FileAdapter>,
    detection_reason: FileMatcher,
    active_adapters: ActiveAdapters,
) -> Result<ReadBox> {
    let meta = adapter.metadata();
    debug!(
        "Chose adapter '{}' because of matcher {:?}",
        &meta.name, &detection_reason
    );
    eprintln!(
        "{} adapter: {}",
        ai.filepath_hint.to_string_lossy(),
        &meta.name
    );
    let cache_compression_level = ai.config.cache.compression_level;
    let cache_max_blob_len = ai.config.cache.max_blob_len;

    let cache: Option<Box<dyn PreprocCache + Send>> = if ai.is_real_file && !ai.config.cache.disabled {
        let daemon_port = ai.config.cache.daemon_port;
        // Check if daemon is alive with a quick timeout
        let daemon_available = tokio::time::timeout(
            std::time::Duration::from_millis(10),
            tokio::net::TcpStream::connect(format!("127.0.0.1:{}", daemon_port))
        ).await.is_ok_and(|res| res.is_ok());

        if daemon_available {
            debug!("Using daemon for caching on port {}", daemon_port);
            Some(Box::new(crate::daemon::DaemonCacheClient::new(daemon_port)))
        } else {
            debug!("Daemon not found on port {}, using local sqlite cache", daemon_port);
            Some(open_cache_db(&ai.config).await?)
        }
    } else {
        None
    };

    if let Some(mut cache) = cache {
        let file_mtime_unix_ms = ai.file_mtime_unix_ms.unwrap_or_else(|| {
            std::fs::metadata(&ai.filepath_hint)
                .and_then(|m| m.modified())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e)))
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0)
        });
        let cache_key = CacheKey::new(
            &ai.filepath_hint,
            file_mtime_unix_ms,
            adapter.as_ref(),
            &active_adapters,
            &ai.config,
        )?;
        
        let cached = cache.get(&cache_key).await.context("cache.get")?;
        match cached {
            Some(cached) => Ok(Box::pin(ZstdDecoder::new(Cursor::new(cached)))),
            None => {
                debug!("cache MISS, running adapter with caching...");
                let inp = loop_adapt(adapter.as_ref(), detection_reason, ai, active_adapters).await?;
                let inp = concat_read_streams(inp);
                let inp = async_read_and_write_to_cache(
                    inp,
                    cache_max_blob_len.0,
                    cache_compression_level.0,
                    Box::new(move |(uncompressed_size, compressed)| {
                        Box::pin(async move {
                            debug!(
                                "uncompressed output: {}",
                                print_bytes(uncompressed_size as f64)
                            );
                            if let Some(cached) = compressed {
                                debug!("compressed output: {}", print_bytes(cached.len() as f64));
                                cache
                                    .set(&cache_key, cached)
                                    .await
                                    .context("writing to cache")?
                            }
                            Ok(())
                        })
                    }),
                )?;

                Ok(Box::pin(inp))
            }
        }
    } else {
        debug!("cache DISABLED, running adapter directly...");
        let inp = loop_adapt(adapter.as_ref(), detection_reason, ai, active_adapters).await?;
        Ok(concat_read_streams(inp))
    }
}

async fn read_discard(mut x: ReadBox) -> Result<()> {
    let mut buf = [0u8; 1 << 16];
    loop {
        let n = x.read(&mut buf).await?;
        if n == 0 {
            break;
        }
    }
    Ok(())
}

pub fn loop_adapt(
    adapter: &dyn FileAdapter,
    detection_reason: FileMatcher,
    ai: AdaptInfo,
    active_adapters: ActiveAdapters,
) -> Pin<Box<dyn Future<Output = anyhow::Result<AdaptedFilesIterBox>> + Send + '_>> {
    Box::pin(async move { loop_adapt_inner(adapter, detection_reason, ai, active_adapters).await })
}
pub async fn loop_adapt_inner(
    adapter: &dyn FileAdapter,
    detection_reason: FileMatcher,
    ai: AdaptInfo,
    active_adapters: ActiveAdapters,
) -> anyhow::Result<AdaptedFilesIterBox> {
    let fph = ai.filepath_hint.clone();
    let inp = adapter.adapt(ai, &detection_reason).await;
    let inp = if adapter.metadata().name == "postprocprefix" {
        // don't add confusing error context
        inp?
    } else {
        inp.with_context(|| {
            format!(
                "adapting {} via {} failed",
                fph.to_string_lossy(),
                adapter.metadata().name
            )
        })?
    };
    let s = stream! {
        for await file in inp {
            trace!("next file");
            match buf_choose_adapter(file?, Some(&active_adapters)).await? {
                Ret::Recurse(ai, adapter, detection_reason, _active_adapters) => {
                    if ai.archive_recursion_depth >= ai.config.max_archive_recursion.0 {
                        // some adapters (esp. zip) assume that the entry is read fully and might hang otherwise
                        read_discard(ai.inp).await?;
                        let s = format!("{}[rga: max archive recursion reached ({})]\n", ai.line_prefix, ai.archive_recursion_depth).into_bytes();
                        yield Ok(AdaptInfo {
                            inp: Box::pin(Cursor::new(s)),
                            ..ai
                        });
                        continue;
                    }
                    debug!(
                        "Chose adapter '{}' because of matcher {:?}",
                        &adapter.metadata().name, &detection_reason
                    );
                    eprintln!(
                        "{} adapter: {}",
                        ai.filepath_hint.to_string_lossy(),
                        &adapter.metadata().name
                    );
                    for await ifile in loop_adapt(adapter.as_ref(), detection_reason, ai, active_adapters.clone()).await? {
                        yield ifile;
                    }
                }
                Ret::Passthrough(ai) => {
                    debug!("no adapter for {}, ending recursion", ai.filepath_hint.to_string_lossy());
                    yield Ok(ai);
                }
            }
            trace!("done with files");
        }
        trace!("stream ended");
    };
    Ok(Box::pin(s))
}
