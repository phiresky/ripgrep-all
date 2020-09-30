use crate::adapters::*;
use crate::{matching::*, recurse::RecursingConcattyReader};
use crate::{
    preproc_cache::{LmdbCache, PreprocCache},
    print_bytes, print_dur, CachingReader,
};
use anyhow::*;
use log::*;
use path_clean::PathClean;
use postproc::PostprocPrefix;
use std::convert::TryInto;

use std::io::{BufRead, BufReader};

use std::{rc::Rc, time::Instant};
/**
 * preprocess a file as defined in `ai`.
 *
 * If a cache is passed, read/write to it.
 *
 */
pub fn rga_preproc(ai: AdaptInfo) -> Result<ReadBox> {
    let AdaptInfo {
        filepath_hint,
        is_real_file,
        inp,
        line_prefix,
        config,
        archive_recursion_depth,
        postprocess,
    } = ai;
    debug!("path (hint) to preprocess: {:?}", filepath_hint);
    let filtered_adapters =
        get_adapters_filtered(config.custom_adapters.clone(), &config.adapters)?;
    let adapters = adapter_matcher(&filtered_adapters, config.accurate)?;
    let filename = filepath_hint
        .file_name()
        .ok_or_else(|| format_err!("Empty filename"))?;
    debug!("Archive recursion depth: {}", archive_recursion_depth);
    if archive_recursion_depth >= config.max_archive_recursion.0 {
        let s = format!("{}[rga: max archive recursion reached]", line_prefix).into_bytes();
        return Ok(Box::new(std::io::Cursor::new(s)));
    }

    // todo: figure out when using a bufreader is a good idea and when it is not
    // seems to be good for File::open() reads, but not sure about within archives (tar, zip)
    let mut inp = BufReader::with_capacity(1 << 16, inp);

    let mimetype = if config.accurate {
        let buf = inp.fill_buf()?; // fill but do not consume!
        let mimetype = tree_magic::from_u8(buf);
        debug!("mimetype: {:?}", mimetype);
        Some(mimetype)
    } else {
        None
    };
    let adapter = adapters(FileMeta {
        mimetype,
        lossy_filename: filename.to_string_lossy().to_string(),
    });
    let (adapter, detection_reason) = match adapter {
        Some((a, d)) => (a, d),
        None => {
            // allow passthrough if the file is in an archive or accurate matching is enabled
            // otherwise it should have been filtered out by rg pre-glob since rg can handle those better than us
            let allow_cat = !is_real_file || config.accurate;
            if allow_cat {
                if postprocess {
                    (
                        Rc::new(PostprocPrefix {}) as Rc<dyn FileAdapter>,
                        FileMatcher::Fast(FastFileMatcher::FileExtension("default".to_string())), // todo: separate enum value for this
                    )
                } else {
                    return Ok(Box::new(inp));
                }
            } else {
                return Err(format_err!(
                    "No adapter found for file {:?}, passthrough disabled.",
                    filename
                ));
            }
        }
    };
    let path_hint_copy = filepath_hint.clone();
    run_adapter(
        AdaptInfo {
            filepath_hint,
            is_real_file,
            inp: Box::new(inp),
            line_prefix,
            config,
            archive_recursion_depth,
            postprocess,
        },
        adapter,
        detection_reason,
        &filtered_adapters,
    )
    .with_context(|| format!("run_adapter({})", &path_hint_copy.to_string_lossy()))
}

fn run_adapter<'a>(
    ai: AdaptInfo<'a>,
    adapter: Rc<dyn FileAdapter>,
    detection_reason: FileMatcher,
    filtered_adapters: &Vec<Rc<dyn FileAdapter>>,
) -> Result<ReadBox<'a>> {
    let AdaptInfo {
        filepath_hint,
        is_real_file,
        inp,
        line_prefix,
        config,
        archive_recursion_depth,
        postprocess,
    } = ai;
    let meta = adapter.metadata();
    debug!(
        "Chose adapter '{}' because of matcher {:?}",
        &meta.name, &detection_reason
    );
    eprintln!(
        "{} adapter: {}",
        filepath_hint.to_string_lossy(),
        &meta.name
    );
    let db_name = format!("{}.v{}", meta.name, meta.version);
    let cache_compression_level = config.cache.compression_level;
    let cache_max_blob_len = config.cache.max_blob_len;

    let cache = if is_real_file {
        LmdbCache::open(&config.cache)?
    } else {
        None
    };

    if let Some(mut cache) = cache {
        let cache_key: Vec<u8> = {
            let clean_path = filepath_hint.to_owned().clean();
            let meta = std::fs::metadata(&filepath_hint).with_context(|| {
                format!("reading metadata for {}", filepath_hint.to_string_lossy())
            })?;
            let modified = meta.modified().expect("weird OS that can't into mtime");

            if adapter.metadata().recurses {
                let key = (
                    filtered_adapters
                        .iter()
                        .map(|a| (a.metadata().name.clone(), a.metadata().version))
                        .collect::<Vec<_>>(),
                    clean_path,
                    modified,
                );
                debug!("Cache key (with recursion): {:?}", key);
                bincode::serialize(&key).expect("could not serialize path")
            } else {
                let key = (
                    adapter.metadata().name.clone(),
                    adapter.metadata().version,
                    clean_path,
                    modified,
                );
                debug!("Cache key (no recursion): {:?}", key);
                bincode::serialize(&key).expect("could not serialize path")
            }
        };
        // let dbg_ctx = format!("adapter {}", &adapter.metadata().name);
        let cached = cache.get(&db_name, &cache_key)?;
        match cached {
            Some(cached) => Ok(Box::new(
                zstd::stream::read::Decoder::new(std::io::Cursor::new(cached))
                    .context("could not create zstd decoder")?,
            )),
            None => {
                debug!("cache MISS, running adapter");
                debug!("adapting with caching...");
                let inp = adapter
                    .adapt(
                        AdaptInfo {
                            line_prefix,
                            filepath_hint: filepath_hint.clone(),
                            is_real_file,
                            inp: Box::new(inp),
                            archive_recursion_depth,
                            config,
                            postprocess,
                        },
                        &detection_reason,
                    )
                    .with_context(|| {
                        format!(
                            "adapting {} via {} failed",
                            filepath_hint.to_string_lossy(),
                            meta.name
                        )
                    })?;
                let inp = RecursingConcattyReader::concat(inp)?;
                let inp = CachingReader::new(
                    inp,
                    cache_max_blob_len.0.try_into().unwrap(),
                    cache_compression_level.0.try_into().unwrap(),
                    Box::new(move |(uncompressed_size, compressed)| {
                        debug!(
                            "uncompressed output: {}",
                            print_bytes(uncompressed_size as f64)
                        );
                        if let Some(cached) = compressed {
                            debug!("compressed output: {}", print_bytes(cached.len() as f64));
                            cache.set(&db_name, &cache_key, &cached)?
                        }
                        Ok(())
                    }),
                )?;

                Ok(Box::new(inp))
            }
        }
    } else {
        // no cache arc - probably within archive
        debug!("adapting without caching...");
        let start = Instant::now();
        let oread = adapter
            .adapt(
                AdaptInfo {
                    line_prefix,
                    filepath_hint: filepath_hint.clone(),
                    is_real_file,
                    inp,
                    archive_recursion_depth,
                    config,
                    postprocess,
                },
                &detection_reason,
            )
            .with_context(|| {
                format!(
                    "adapting {} via {} without caching failed",
                    filepath_hint.to_string_lossy(),
                    meta.name
                )
            })?;
        debug!(
            "running adapter {} took {}",
            adapter.metadata().name,
            print_dur(start)
        );
        Ok(RecursingConcattyReader::concat(oread)?)
    }
}
