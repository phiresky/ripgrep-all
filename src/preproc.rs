use crate::adapted_iter::AdaptedFilesIterBox;
use crate::adapters::*;
use crate::caching_writer::async_read_and_write_to_cache;
use crate::config::RgaConfig;
use crate::matching::*;
use crate::preproc_cache::CacheKey;
use crate::recurse::concat_read_streams;
use crate::{
    preproc_cache::{open_cache_db, PreprocCache},
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
) -> Result<Option<(Arc<dyn FileAdapter>, FileMatcher, ActiveAdapters)>> {
    let active_adapters = get_adapters_filtered(
        config.custom_identifiers.clone(),
        config.custom_adapters.clone(),
        &config.adapters,
    )?;
    let adapters = adapter_matcher(&active_adapters, config.accurate)?;
    let filename = filepath_hint
        .file_name()
        .ok_or_else(|| format_err!("Empty filename"))?;
    debug!("Archive recursion depth: {}", archive_recursion_depth);

    let mimetype = if config.accurate {
        let buf = inp.fill_buf().await?; // fill but do not consume!
        if buf.starts_with(b"From \x0d") || buf.starts_with(b"From -") {
            Some("application/mbox")
        } else {
            let mimetype = tree_magic::from_u8(buf);
            debug!("mimetype: {:?}", mimetype);
            Some(mimetype)
        }
    } else {
        None
    };
    let adapter = adapters(FileMeta {
        mimetype,
        lossy_filename: filename.to_string_lossy().to_string(),
    });
    Ok(adapter.map(|e| (e.0, e.1, active_adapters)))
}

enum Ret {
    Recurse(AdaptInfo, Arc<dyn FileAdapter>, FileMatcher, ActiveAdapters),
    Passthrough(AdaptInfo),
}
async fn buf_choose_adapter(ai: AdaptInfo) -> Result<Ret> {
    let mut inp = BufReader::with_capacity(1 << 16, ai.inp);
    let adapter = choose_adapter(
        &ai.config,
        &ai.filepath_hint,
        ai.archive_recursion_depth,
        &mut inp,
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
    let (ai, adapter, detection_reason, active_adapters) = match buf_choose_adapter(ai).await? {
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

    let cache = if ai.is_real_file && !ai.config.cache.disabled {
        Some(open_cache_db(Path::new(&ai.config.cache.path.0)).await?)
    } else {
        None
    };

    let mut cache = cache.context("No cache?")?;
    let cache_key = CacheKey::new(
        ai.postprocess,
        &ai.filepath_hint,
        adapter.as_ref(),
        &active_adapters,
    )?;
    // let dbg_ctx = format!("adapter {}", &adapter.metadata().name);
    let cached = cache.get(&cache_key).await.context("cache.get")?;
    match cached {
        Some(cached) => Ok(Box::pin(ZstdDecoder::new(Cursor::new(cached)))),
        None => {
            debug!("cache MISS, running adapter with caching...");
            let inp = loop_adapt(adapter.as_ref(), detection_reason, ai).await?;
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
) -> Pin<Box<dyn Future<Output = anyhow::Result<AdaptedFilesIterBox>> + Send + '_>> {
    Box::pin(async move { loop_adapt_inner(adapter, detection_reason, ai).await })
}
pub async fn loop_adapt_inner(
    adapter: &dyn FileAdapter,
    detection_reason: FileMatcher,
    ai: AdaptInfo,
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
            match buf_choose_adapter(file?).await? {
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
                    for await ifile in loop_adapt(adapter.clone().as_ref(), detection_reason, ai).await? {
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
