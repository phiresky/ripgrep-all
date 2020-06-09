use crate::adapters::*;
use crate::args::RgaConfig;
use crate::matching::*;
use crate::{print_bytes, print_dur, CachingWriter};
use anyhow::*;
use log::*;
use path_clean::PathClean;
use std::convert::TryInto;
use std::io::BufRead;
use std::io::BufReader;
use std::io::BufWriter;
use std::{
    sync::{Arc, RwLock},
    time::Instant,
};

#[derive(Clone)]
pub struct PreprocConfig<'a> {
    pub cache: Option<Arc<RwLock<dyn crate::preproc_cache::PreprocCache>>>,
    pub args: &'a RgaConfig,
}
/**
 * preprocess a file as defined in `ai`.
 *
 * If a cache is passed, read/write to it.
 *
 */
pub fn rga_preproc(ai: AdaptInfo) -> Result<()> {
    let AdaptInfo {
        filepath_hint,
        is_real_file,
        inp,
        oup,
        line_prefix,
        config,
        archive_recursion_depth,
        ..
    } = ai;
    debug!("path (hint) to preprocess: {:?}", filepath_hint);
    let PreprocConfig { mut cache, args } = config;
    let filtered_adapters = get_adapters_filtered(args.custom_adapters.clone(), &args.adapters)?;
    let adapters = adapter_matcher(&filtered_adapters, args.accurate)?;
    let filename = filepath_hint
        .file_name()
        .ok_or_else(|| format_err!("Empty filename"))?;
    debug!("Archive recursion depth: {}", archive_recursion_depth);
    if archive_recursion_depth >= args.max_archive_recursion.0 {
        writeln!(oup, "{}[rga: max archive recursion reached]", line_prefix)?;
        return Ok(());
    }

    // todo: figure out when using a bufreader is a good idea and when it is not
    // seems to be good for File::open() reads, but not sure about within archives (tar, zip)
    let inp = &mut BufReader::with_capacity(1 << 13, inp);

    let mimetype = if args.accurate {
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
    match adapter {
        Some((adapter, detection_reason)) => {
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
            if let Some(cache) = cache.as_mut() {
                let cache_key: Vec<u8> = {
                    let clean_path = filepath_hint.to_owned().clean();
                    let meta = std::fs::metadata(&filepath_hint)?;
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
                cache.write().unwrap().get_or_run(
                    &db_name,
                    &cache_key,
                    &adapter.metadata().name,
                    Box::new(|| -> Result<Option<Vec<u8>>> {
                        // wrapping BufWriter here gives ~10% perf boost
                        let mut compbuf = BufWriter::new(CachingWriter::new(
                            oup,
                            args.cache_max_blob_len.0.try_into().unwrap(),
                            args.cache_compression_level.0.try_into().unwrap(),
                        )?);
                        debug!("adapting with caching...");
                        adapter
                            .adapt(
                                AdaptInfo {
                                    line_prefix,
                                    filepath_hint,
                                    is_real_file,
                                    inp,
                                    oup: &mut compbuf,
                                    archive_recursion_depth,
                                    config: PreprocConfig { cache: None, args },
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
                        let (uncompressed_size, compressed) = compbuf
                            .into_inner()
                            .map_err(|_| anyhow!("could not finish zstd"))? // can't use with_context here
                            .finish()?;
                        debug!(
                            "uncompressed output: {}",
                            print_bytes(uncompressed_size as f64)
                        );
                        if let Some(cached) = compressed {
                            debug!("compressed output: {}", print_bytes(cached.len() as f64));
                            Ok(Some(cached))
                        } else {
                            Ok(None)
                        }
                    }),
                    Box::new(|cached| {
                        let stdouti = std::io::stdout();
                        zstd::stream::copy_decode(cached, stdouti.lock())?;
                        Ok(())
                    }),
                )?;
                Ok(())
            } else {
                // no cache arc - probably within archive
                debug!("adapting without caching...");
                let start = Instant::now();
                adapter
                    .adapt(
                        AdaptInfo {
                            line_prefix,
                            filepath_hint,
                            is_real_file,
                            inp,
                            oup,
                            archive_recursion_depth,
                            config: PreprocConfig { cache: None, args },
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
                Ok(())
            }
        }
        None => {
            // allow passthrough if the file is in an archive or accurate matching is enabled
            // otherwise it should have been filtered out by rg pre-glob since rg can handle those better than us
            let allow_cat = !is_real_file || args.accurate;
            if allow_cat {
                spawning::postproc_line_prefix(line_prefix, inp, oup)?;
                Ok(())
            } else {
                Err(format_err!(
                    "No adapter found for file {:?}, passthrough disabled.",
                    filename
                ))
            }
        }
    }
}
