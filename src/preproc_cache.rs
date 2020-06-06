use anyhow::{format_err, Context, Result};
use log::*;
use std::{
    fmt::Display,
    sync::{Arc, RwLock},
};

pub fn open() -> Result<Arc<RwLock<dyn PreprocCache>>> {
    Ok(Arc::new(RwLock::new(LmdbCache::open()?)))
}
pub trait PreprocCache {
    // possible without second lambda?
    fn get_or_run<'a>(
        &mut self,
        db_name: &str,
        key: &[u8],
        runner: Box<dyn FnOnce() -> Result<Option<Vec<u8>>> + 'a>,
        callback: Box<dyn FnOnce(&[u8]) -> Result<()> + 'a>,
    ) -> Result<()>;
}

/// opens a LMDB cache
fn open_cache_db() -> Result<std::sync::Arc<std::sync::RwLock<rkv::Rkv>>> {
    let app_cache = cachedir::CacheDirConfig::new("rga").get_cache_dir()?;

    let db_arc = rkv::Manager::singleton()
        .write()
        .expect("could not write db manager")
        .get_or_create(app_cache.as_path(), |p| {
            let mut builder = rkv::Rkv::environment_builder();
            builder
                .set_flags(rkv::EnvironmentFlags::NO_SYNC | rkv::EnvironmentFlags::WRITE_MAP) // not durable cuz it's a cache
                // i'm not sure why NO_TLS is needed. otherwise LMDB transactions (open readers) will keep piling up until it fails with
                // LmdbError(ReadersFull). Those "open readers" stay even after the corresponding processes exit.
                // hope setting this doesn't break integrity
                .set_flags(rkv::EnvironmentFlags::NO_TLS)
                // sometimes, this seems to cause the data.mdb file to appear as 2GB in size (with holes), but sometimes not?
                .set_map_size(2 * 1024 * 1024 * 1024)
                .set_max_dbs(100)
                .set_max_readers(128);
            rkv::Rkv::from_env(p, builder)
        })
        .expect("could not get/create db");
    Ok(db_arc)
}

pub struct LmdbCache {
    db_arc: std::sync::Arc<std::sync::RwLock<rkv::Rkv>>,
}

impl LmdbCache {
    pub fn open() -> Result<LmdbCache> {
        Ok(LmdbCache {
            db_arc: open_cache_db()?,
        })
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
    // possible without second lambda?
    fn get_or_run<'a>(
        &mut self,
        db_name: &str,
        key: &[u8],
        runner: Box<dyn FnOnce() -> Result<Option<Vec<u8>>> + 'a>,
        callback: Box<dyn FnOnce(&[u8]) -> Result<()> + 'a>,
    ) -> Result<()> {
        let db_env = self.db_arc.read().unwrap();
        let db = db_env
            .open_single(db_name, rkv::store::Options::create())
            .map_err(RkvErrWrap)
            .with_context(|| format_err!("could not open cache db store"))?;

        let reader = db_env.read().expect("could not get reader");
        let cached = db
            .get(&reader, &key)
            .map_err(RkvErrWrap)
            .with_context(|| format_err!("could not read from db"))?;

        match cached {
            Some(rkv::Value::Blob(cached)) => {
                debug!("got cached");
                callback(cached)?;
            }
            Some(_) => Err(format_err!("Integrity: value not blob"))?,
            None => {
                debug!("did not get cached");
                drop(reader);
                if let Some(got) = runner()? {
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
                }
            }
        };
        Ok(())
    }
}
