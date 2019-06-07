use crate::adapters::*;
use crate::CachingWriter;
use failure::Fallible;
use failure::{format_err, Error};
use path_clean::PathClean;
use std::io::BufWriter;
// longest compressed conversion output to save in cache
const MAX_DB_BLOB_LEN: usize = 2_000_000;
const ZSTD_LEVEL: i32 = 12;
use std::sync::{Arc, RwLock};

#[derive(Clone)]
pub struct PreprocConfig {
    pub cache: Option<Arc<RwLock<dyn crate::preproc_cache::PreprocCache>>>,
    pub max_archive_recursion: i32,
}
/**
 * preprocess a file as defined in `ai`.
 *
 * If a cache is passed, read/write to it.
 *
 */
pub fn rga_preproc(ai: AdaptInfo) -> Result<(), Error> {
    let adapters = adapter_matcher()?;
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
    let PreprocConfig {
        mut cache,
        max_archive_recursion,
    } = config;
    let filename = filepath_hint
        .file_name()
        .ok_or_else(|| format_err!("Empty filename"))?;
    eprintln!("depth: {}", archive_recursion_depth);
    if archive_recursion_depth >= config.max_archive_recursion {
        writeln!(oup, "{}[rga: max archive recursion reached]", line_prefix)?;
        return Ok(());
    }

    eprintln!("path_hint: {:?}", filepath_hint);

    /*let mimetype = tree_magic::from_filepath(path).ok_or(lerr(format!(
        "File {} does not exist",
        filename.to_string_lossy()
    )))?;
    println!("mimetype: {:?}", mimetype);*/
    let adapter = adapters(FileMeta {
        // mimetype,
        lossy_filename: filename.to_string_lossy().to_string(),
    });
    match adapter {
        Some(ad) => {
            let meta = ad.metadata();
            eprintln!("adapter: {}", &meta.name);
            let db_name = format!("{}.v{}", meta.name, meta.version);
            if let Some(cache) = cache.as_mut() {
                let cache_key: Vec<u8> = {
                    let clean_path = filepath_hint.to_owned().clean();
                    let meta = std::fs::metadata(&filepath_hint)?;

                    let key = (
                        clean_path,
                        meta.modified().expect("weird OS that can't into mtime"),
                    );
                    eprintln!("cache key: {:?}", key);

                    bincode::serialize(&key).expect("could not serialize path") // key in the cache database
                };
                cache.write().unwrap().get_or_run(
                    &db_name,
                    &cache_key,
                    Box::new(|| -> Fallible<Option<Vec<u8>>> {
                        // wrapping BufWriter here gives ~10% perf boost
                        let mut compbuf =
                            BufWriter::new(CachingWriter::new(oup, MAX_DB_BLOB_LEN, ZSTD_LEVEL)?);
                        eprintln!("adapting...");
                        ad.adapt(AdaptInfo {
                            line_prefix,
                            filepath_hint,
                            is_real_file,
                            inp,
                            oup: &mut compbuf,
                            archive_recursion_depth,
                            config: PreprocConfig {
                                cache: None,
                                max_archive_recursion,
                            },
                        })?;
                        let compressed = compbuf
                            .into_inner()
                            .map_err(|_| "could not finish zstd")
                            .unwrap()
                            .finish()?;
                        if let Some(cached) = compressed {
                            eprintln!("compressed len: {}", cached.len());
                        };
                        Ok(None)
                    }),
                    Box::new(|cached| {
                        let stdouti = std::io::stdout();
                        zstd::stream::copy_decode(cached, stdouti.lock())?;
                        Ok(())
                    }),
                )?;
                Ok(())
            } else {
                eprintln!("adapting...");
                ad.adapt(AdaptInfo {
                    line_prefix,
                    filepath_hint,
                    is_real_file,
                    inp,
                    oup,
                    archive_recursion_depth,
                    config: PreprocConfig {
                        cache: None,
                        max_archive_recursion,
                    },
                })?;
                Ok(())
            }
        }
        None => {
            // allow passthrough if the file is in an archive,
            // otherwise it should have been filtered out by rg pre-glob since rg can handle those better than us
            let allow_cat = !is_real_file;
            if allow_cat {
                spawning::postproc_line_prefix(line_prefix, inp, oup)?;
                Ok(())
            } else {
                Err(format_err!("No adapter found for file {:?}", filename))
            }
        }
    }
}
