use crate::adapted_iter::AdaptedFilesIterBox;
use crate::adapters::*;
use crate::caching_writer::async_read_and_write_to_cache;
use crate::caching_writer::load_zstd_dict_path;
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
use std::collections::HashMap;
use std::sync::Mutex;
use lazy_static::lazy_static;

pub type ActiveAdapters = Vec<Arc<dyn FileAdapter>>;

type MatcherFn = dyn Fn(FileMeta) -> Option<(Arc<dyn FileAdapter>, FileMatcher)> + Send + Sync;

#[derive(Clone)]
pub struct AdapterEngine {
    pub active_adapters: ActiveAdapters,
    pub matcher: std::sync::Arc<MatcherFn>,
}

pub fn make_engine(config: &RgaConfig) -> Result<AdapterEngine> {
    if let Some(p) = &config.cache.dict_path {
        let _ = load_zstd_dict_path(p);
    }
    let active_adapters = get_adapters_filtered(config.custom_adapters.clone(), &config.adapters)?;
    let matcher_box = adapter_matcher(&active_adapters, config.accurate)?;
    let matcher: std::sync::Arc<MatcherFn> = matcher_box.into();
    Ok(AdapterEngine { active_adapters, matcher })
}

#[derive(Default, Clone)]
struct AdapterStat { runs: u64, cache_hits: u64, cache_misses: u64, bytes_uncompressed: u64, bytes_compressed: u64 }
lazy_static! { static ref PROF_STATS: Mutex<HashMap<String, AdapterStat>> = Mutex::new(HashMap::new()); }
fn prof_hit(adapter: &str) {
    let mut g = PROF_STATS.lock().unwrap();
    let e = g.entry(adapter.to_string()).or_default();
    e.runs += 1; e.cache_hits += 1;
}
fn prof_miss(adapter: &str, uncompressed: u64, compressed: Option<usize>) {
    let mut g = PROF_STATS.lock().unwrap();
    let e = g.entry(adapter.to_string()).or_default();
    e.runs += 1; e.cache_misses += 1; e.bytes_uncompressed += uncompressed; if let Some(c) = compressed { e.bytes_compressed += c as u64; }
}
pub fn prof_summary() -> String {
    let g = PROF_STATS.lock().unwrap();
    let mut v: Vec<(String, AdapterStat)> = g.iter().map(|(k, s)| (k.clone(), s.clone())).collect();
    v.sort_by_key(|e| e.0.clone());
    let mut out = String::new();
    for (name, s) in v.into_iter() {
        out.push_str(&format!("{}: runs={}, hits={}, misses={}, bytes={}, compressed={}\n", name, s.runs, s.cache_hits, s.cache_misses, s.bytes_uncompressed, s.bytes_compressed));
    }
    if let Some((gz,bz,xz,z)) = crate::adapters::decompress::tuned_caps() {
        out.push_str(&format!("decompressor caps: gzip={}, bzip2={}, xz={}, zstd={}\n", gz, bz, xz, z));
    }
    out
}

async fn choose_adapter(
    engine: &AdapterEngine,
    config: &RgaConfig,
    filepath_hint: &Path,
    archive_recursion_depth: i32,
    inp: &mut (impl AsyncBufRead + Unpin),
) -> Result<Option<(Arc<dyn FileAdapter>, FileMatcher, ActiveAdapters)>> {
    let filename = filepath_hint
        .file_name()
        .ok_or_else(|| format_err!("Empty filename"))?;
    debug!("Archive recursion depth: {}", archive_recursion_depth);

    let first = (engine.matcher)(FileMeta {
        mimetype: None,
        lossy_filename: filename.to_string_lossy().to_string(),
    });
    if first.is_some() {
        return Ok(first.map(|e| (e.0, e.1, engine.active_adapters.clone())));
    }
    let mimetype = if config.accurate {
        let buf = inp.fill_buf().await?;
        let probe = &buf[..buf.len().min(8192)];
        if probe.starts_with(b"From \x0d") || probe.starts_with(b"From -") {
            Some("application/mbox")
        } else {
            let mimetype = tree_magic::from_u8(probe);
            debug!("mimetype: {:?}", mimetype);
            Some(mimetype)
        }
    } else {
        None
    };
    let second = (engine.matcher)(FileMeta {
        mimetype,
        lossy_filename: filename.to_string_lossy().to_string(),
    });
    Ok(second.map(|e| (e.0, e.1, engine.active_adapters.clone())))
}

enum Ret {
    Recurse(AdaptInfo, Arc<dyn FileAdapter>, FileMatcher, ActiveAdapters),
    Passthrough(AdaptInfo),
}
async fn buf_choose_adapter(engine: &AdapterEngine, ai: AdaptInfo) -> Result<Ret> {
    let mut inp = BufReader::with_capacity(1 << 16, ai.inp);
    let adapter = choose_adapter(
        engine,
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
                if ai.postprocess && !ai.config.no_prefix_filenames && !ai.config.no_prefix_for_adapters.iter().any(|s| s == "default" || s == "*") {
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
    let engine = make_engine(&ai.config)?;
    if let Some(maxb) = ai.config.max_file_bytes
        && ai.is_real_file
        && let std::result::Result::Ok(md) = std::fs::metadata(&ai.filepath_hint)
        && md.len() as usize > maxb {
        let allow_cat = !ai.config.no_prefix_filenames && ai.postprocess && !ai.config.no_prefix_for_adapters.iter().any(|s| s == "default" || s == "*");
        if allow_cat {
            let ai2 = AdaptInfo { inp: ai.inp, filepath_hint: ai.filepath_hint, is_real_file: ai.is_real_file, archive_recursion_depth: ai.archive_recursion_depth, line_prefix: ai.line_prefix, config: ai.config.clone(), postprocess: false };
            let path_hint_copy = ai2.filepath_hint.clone();
            return adapt_caching(&engine, ai2, Arc::new(PostprocPrefix {}), FileMatcher::Fast(FastFileMatcher::FileExtension("default".to_string())), vec![]).await.with_context(|| format!("run_adapter({})", &path_hint_copy.to_string_lossy()));
        } else {
            return Ok(ai.inp);
        }
    }

    // todo: figure out when using a bufreader is a good idea and when it is not
    // seems to be good for File::open() reads, but not sure about within archives (tar, zip)
    let (ai, adapter, detection_reason, active_adapters) = match buf_choose_adapter(&engine, ai).await? {
        Ret::Recurse(ai, a, b, c) => (ai, a, b, c),
        Ret::Passthrough(ai) => {
            return Ok(ai.inp);
        }
    };
    let path_hint_copy = ai.filepath_hint.clone();
    adapt_caching(&engine, ai, adapter, detection_reason, active_adapters)
        .await
        .with_context(|| format!("run_adapter({})", &path_hint_copy.to_string_lossy()))
}

async fn adapt_caching(
    engine: &AdapterEngine,
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
    debug!(
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
    if cache.is_none() {
        let inp = loop_adapt(engine.clone(), adapter.as_ref(), detection_reason, ai).await?;
        let inp = concat_read_streams(inp);
        return Ok(Box::pin(inp));
    }
    let mut cache = cache.unwrap();
    let cache_key = CacheKey::new(
        ai.postprocess,
        &ai.filepath_hint,
        adapter.as_ref(),
        &active_adapters,
    )?;
    // let dbg_ctx = format!("adapter {}", &adapter.metadata().name);
    let cached = cache.get(&cache_key).await.context("cache.get")?;
    match cached {
        Some(cached) => {
            if ai.config.profile { prof_hit(&adapter.metadata().name); }
            Ok(Box::pin(ZstdDecoder::new(Cursor::new(cached))))
        },
        None => {
            debug!("cache MISS, running adapter with caching...");
            let profile_enabled = ai.config.profile;
            let adapter_name = adapter.metadata().name.clone();
            let small_uncompressed = if ai.config.cache.disable_small_uncompressed { 0 } else { ai.config.cache.small_uncompressed_bytes.0 };
            let inp = loop_adapt(engine.clone(), adapter.as_ref(), detection_reason, ai).await?;
            let inp = concat_read_streams(inp);
            let inp = async_read_and_write_to_cache(
                inp,
                cache_max_blob_len.0,
                cache_compression_level.0,
                small_uncompressed,
                Box::new(move |(uncompressed_size, compressed)| {
                    Box::pin(async move {
                        debug!(
                            "uncompressed output: {}",
                            print_bytes(uncompressed_size as f64)
                        );
                        if let Some(ref cached) = compressed {
                            debug!("compressed output: {}", print_bytes(cached.len() as f64));
                            cache
                                .set(&cache_key, cached.to_vec())
                                .await
                                .context("writing to cache")?
                        }
                        if profile_enabled { prof_miss(&adapter_name, uncompressed_size, compressed.as_ref().map(|c| c.len())); }
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
    engine: AdapterEngine,
    adapter: &dyn FileAdapter,
    detection_reason: FileMatcher,
    ai: AdaptInfo,
) -> Pin<Box<dyn Future<Output = anyhow::Result<AdaptedFilesIterBox>> + Send + '_>> {
    Box::pin(async move { loop_adapt_inner(engine, adapter, detection_reason, ai).await })
}
pub async fn loop_adapt_inner(
    engine: AdapterEngine,
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
            match buf_choose_adapter(&engine, file?).await? {
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
                    debug!(
                        "{} adapter: {}",
                        ai.filepath_hint.to_string_lossy(),
                        &adapter.metadata().name
                    );
                    let engine_c = engine.clone();
                    for await ifile in loop_adapt(engine_c, adapter.as_ref(), detection_reason, ai).await? {
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
