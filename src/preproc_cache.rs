use crate::{config::CacheConfig, print_bytes, print_dur};
use anyhow::{format_err, Context, Result};
use log::*;
use rkv::backend::{BackendEnvironmentBuilder, LmdbEnvironment};
use std::{fmt::Display, path::Path, time::Instant};

pub trait PreprocCache: Send + Sync {
    /*/// gets cache at specified key.
    /// if cache hit, return the resulting data
    /// else, run the given lambda, and store its result in the cache if present
    fn get_or_run<'a>(
        &mut self,
        db_name: &str,
        key: &[u8],
        debug_name: &str,
        runner: Box<dyn FnOnce() -> Result<Option<Vec<u8>>> + 'a>,
    ) -> Result<Option<Vec<u8>>>;*/

    fn get(&self, db_name: &str, key: &[u8]) -> Result<Option<Vec<u8>>>;
    fn set(&mut self, db_name: &str, key: &[u8], value: &[u8]) -> Result<()>;
}

/// opens a LMDB cache
fn open_cache_db(
    path: &Path,
) -> Result<std::sync::Arc<std::sync::RwLock<rkv::Rkv<LmdbEnvironment>>>> {
    std::fs::create_dir_all(path)?;
    use rkv::backend::LmdbEnvironmentFlags;

    rkv::Manager::<LmdbEnvironment>::singleton()
        .write()
        .map_err(|_| format_err!("could not write cache db manager"))?
        .get_or_create(path, |p| {
            let mut builder = rkv::Rkv::environment_builder::<rkv::backend::Lmdb>();
            builder
                .set_flags(rkv::EnvironmentFlags::NO_SYNC)
                .set_flags(rkv::EnvironmentFlags::WRITE_MAP) // not durable cuz it's a cache
                // i'm not sure why NO_TLS is needed. otherwise LMDB transactions (open readers) will keep piling up until it fails with
                // LmdbError(ReadersFull). Those "open readers" stay even after the corresponding processes exit.
                // hope setting this doesn't break integrity
                .set_flags(rkv::EnvironmentFlags::NO_TLS)
                // sometimes, this seems to cause the data.mdb file to appear as 2GB in size (with holes), but sometimes not?
                .set_map_size(2 * 1024 * 1024 * 1024)
                .set_max_dbs(100)
                .set_max_readers(128);
            rkv::Rkv::from_builder(p, builder)
        })
        .map_err(|e| format_err!("could not get/create cache db: {}", e))
}

pub struct LmdbCache {
    db_arc: std::sync::Arc<std::sync::RwLock<rkv::Rkv<LmdbEnvironment>>>,
}

impl LmdbCache {
    pub fn open(config: &CacheConfig) -> Result<Option<LmdbCache>> {
        if config.disabled {
            return Ok(None);
        }
        let path = Path::new(&config.path.0);
        Ok(Some(LmdbCache {
            db_arc: open_cache_db(&path)?,
        }))
    }
}

#[derive(Debug)]
struct RkvErrWrap(rkv::StoreError);
impl Display for RkvErrWrap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for RkvErrWrap {}

impl PreprocCache for LmdbCache {
    fn get(&self, db_name: &str, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let start = Instant::now();
        let db_env = self
            .db_arc
            .read()
            .map_err(|_| anyhow::anyhow!("Could not open lock, some lock writer panicked"))?;
        let db = db_env
            .open_single(db_name, rkv::store::Options::create())
            .map_err(RkvErrWrap)
            .context("could not open cache db store")?;

        let reader = db_env.read().expect("could not get reader");
        let cached = db
            .get(&reader, &key)
            .map_err(RkvErrWrap)
            .context("could not read from db")?;

        match cached {
            Some(rkv::Value::Blob(cached)) => {
                debug!(
                    "cache HIT, reading {} (compressed) from cache",
                    print_bytes(cached.len() as f64)
                );
                debug!("reading from cache took {}", print_dur(start));
                Ok(Some(Vec::from(cached)))
            }
            Some(_) => Err(format_err!("Integrity: value not blob"))?,
            None => Ok(None),
        }
    }
    fn set(&mut self, db_name: &str, key: &[u8], got: &[u8]) -> Result<()> {
        let start = Instant::now();
        debug!("writing {} to cache", print_bytes(got.len() as f64));
        let db_env = self
            .db_arc
            .read()
            .map_err(|_| anyhow::anyhow!("Could not open lock, some lock writer panicked"))?;

        let db = db_env
            .open_single(db_name, rkv::store::Options::create())
            .map_err(RkvErrWrap)
            .context("could not open cache db store")?;

        let mut writer = db_env
            .write()
            .map_err(RkvErrWrap)
            .with_context(|| format_err!("could not open write handle to cache"))?;

        db.put(&mut writer, &key, &rkv::Value::Blob(&got))
            .map_err(RkvErrWrap)
            .with_context(|| format_err!("could not write to cache"))?;
        writer
            .commit()
            .map_err(RkvErrWrap)
            .with_context(|| format!("could not write cache"))?;
        debug!("writing to cache took {}", print_dur(start));
        Ok(())
    }
}
