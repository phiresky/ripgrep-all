use crate::adapters::*;
use crate::matching::*;
use crate::{
    preproc_cache::{LmdbCache, PreprocCache},
    print_bytes, print_dur, CachingReader,
};
use anyhow::*;
use log::*;
use owning_ref::OwningRefMut;
use path_clean::PathClean;
use std::{convert::TryInto, io::Read};

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
        ..
    } = ai;
    debug!("path (hint) to preprocess: {:?}", filepath_hint);
    let filtered_adapters =
        get_adapters_filtered(/*config.custom_adapters.clone(),*/ &config.adapters)?;
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
    match adapter {
        Some((adapter, detection_reason)) => run_adapter(
            AdaptInfo {
                filepath_hint,
                is_real_file,
                inp: Box::new(inp),
                line_prefix,
                config,
                archive_recursion_depth,
            },
            adapter,
            detection_reason,
            &filtered_adapters,
        ),
        None => {
            // allow passthrough if the file is in an archive or accurate matching is enabled
            // otherwise it should have been filtered out by rg pre-glob since rg can handle those better than us
            let allow_cat = !is_real_file || config.accurate;
            if allow_cat {
                Ok(Box::new(inp))
            } else {
                Err(format_err!(
                    "No adapter found for file {:?}, passthrough disabled.",
                    filename
                ))
            }
        }
    }
}

struct ConcattyReader<'a> {
    inp: Box<dyn ReadIter + 'a>,
    cur: Option<AdaptInfo<'a>>,
}
impl<'a> ConcattyReader<'a> {
    fn ascend(&mut self) {
        self.cur = unsafe {
            // would love to make this safe, but how?
            let r: *mut Box<dyn ReadIter + 'a> = &mut self.inp;
            (*r).next()
        };
        eprintln!(
            "ascended to {}",
            self.cur
                .as_ref()
                .map(|e| e.filepath_hint.to_string_lossy().into_owned())
                .unwrap_or("END".to_string())
        );
    }
}
impl<'a> Read for ConcattyReader<'a> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match &mut self.cur {
            None => Ok(0), // last file ended
            Some(cur) => match cur.inp.read(buf) {
                Err(e) => Err(e),
                Ok(0) => {
                    // current file ended, go to next file
                    self.ascend();
                    self.read(buf)
                }
                Ok(n) => Ok(n),
            },
        }
    }
}
fn concattyreader<'a>(inp: Box<dyn ReadIter + 'a>) -> Box<dyn Read + 'a> {
    let mut r = ConcattyReader { inp, cur: None };
    r.ascend();
    Box::new(r)
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
        ..
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

    if let Some(mut cache) = LmdbCache::open(&config.cache)? {
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
                while let Some(innerinp) = inp.next() {}
                /*let inp = concattyreader(inp);
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
                )?;*/

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
        Ok(concattyreader(oread))
    }
}
