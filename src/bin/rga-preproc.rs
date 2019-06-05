use path_clean::PathClean;
use rga::adapters::*;
use rga::CachingWriter;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};
use tree_magic;

const max_db_blob_len: usize = 2000000;

// lazy error
fn lerr(inp: impl AsRef<str>) -> Box<dyn Error> {
    return inp.as_ref().into();
}

fn open_db() -> Result<std::sync::Arc<std::sync::RwLock<rkv::Rkv>>, Box<dyn Error>> {
    let app_cache = cachedir::CacheDirConfig::new("rga").get_cache_dir()?;

    let db_arc = rkv::Manager::singleton()
        .write()
        .expect("could not write db manager")
        .get_or_create(app_cache.as_path(), |p| {
            let mut builder = rkv::Rkv::environment_builder();
            builder
                .set_flags(rkv::EnvironmentFlags::NO_SYNC | rkv::EnvironmentFlags::WRITE_MAP) // not durable
                .set_map_size(2 * 1024 * 1024 * 1024)
                .set_max_dbs(100);
            rkv::Rkv::from_env(p, builder)
        })
        .expect("could not get/create db");
    Ok(db_arc)
}

fn main() -> Result<(), Box<dyn Error>> {
    //db.
    let adapters = init_adapters()?;
    let filepath = std::env::args()
        .skip(1)
        .next()
        .ok_or(lerr("No filename specified"))?;
    eprintln!("fname: {}", filepath);
    let path = PathBuf::from(&filepath);
    let serialized_path: Vec<u8> =
        bincode::serialize(&path.clean()).expect("could not serialize path");
    let filename = path.file_name().ok_or(lerr("Empty filename"))?;

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
            let db_arc = open_db()?;
            let db_env = db_arc.read().unwrap();
            let db = db_env
                .open_single(db_name.as_str(), rkv::store::Options::create())
                .map_err(|p| lerr(format!("could not open db store: {:?}", p)))?;
            let reader = db_env.read().expect("could not get reader");
            match db
                .get(&reader, &serialized_path)
                .map_err(|p| lerr(format!("could not read from db: {:?}", p)))?
            {
                Some(rkv::Value::Blob(cached)) => {
                    let stdouti = std::io::stdout();
                    zstd::stream::copy_decode(cached, stdouti.lock())?;
                    Ok(())
                }
                Some(_) => Err(lerr("Integrity: value not blob")),
                None => {
                    let stdouti = std::io::stdout();
                    let mut compbuf = CachingWriter::new(stdouti.lock(), max_db_blob_len, 12)?;
                    ad.adapt(&filepath, &mut compbuf)?;
                    let compressed = compbuf.finish()?;
                    if let Some(cached) = compressed {
                        eprintln!("compressed len: {}", cached.len());

                        {
                            let mut writer = db_env.write().map_err(|p| {
                                lerr(format!("could not open write handle to cache: {:?}", p))
                            })?;
                            db.put(&mut writer, &serialized_path, &rkv::Value::Blob(&cached))
                                .map_err(|p| lerr(format!("could not write to cache: {:?}", p)))?;
                            writer.commit().unwrap();
                        }
                    }
                    Ok(())
                }
            }
        }
        None => {
            eprintln!("no adapter for that file, running cat!");
            let stdini = std::io::stdin();
            let mut stdin = stdini.lock();
            let stdouti = std::io::stdout();
            let mut stdout = stdouti.lock();
            std::io::copy(&mut stdin, &mut stdout)?;
            Ok(())
        }
    }
}
