use failure::{format_err, Fallible};
use log::*;
use std::sync::{Arc, RwLock};

pub fn open() -> Fallible<Arc<RwLock<dyn PreprocCache>>> {
    Ok(Arc::new(RwLock::new(LmdbCache::open()?)))
}
pub trait PreprocCache {
    // possible without second lambda?
    fn get_or_run<'a>(
        &mut self,
        db_name: &str,
        key: &[u8],
        runner: Box<dyn FnOnce() -> Fallible<Option<Vec<u8>>> + 'a>,
        callback: Box<dyn FnOnce(&[u8]) -> Fallible<()> + 'a>,
    ) -> Fallible<()>;
}

/// opens a LMDB cache
fn open_cache_db() -> Fallible<std::sync::Arc<std::sync::RwLock<rkv::Rkv>>> {
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
    pub fn open() -> Fallible<LmdbCache> {
        Ok(LmdbCache {
            db_arc: open_cache_db()?,
        })
    }
}
impl PreprocCache for LmdbCache {
    // possible without second lambda?
    fn get_or_run<'a>(
        &mut self,
        db_name: &str,
        key: &[u8],
        runner: Box<dyn FnOnce() -> Fallible<Option<Vec<u8>>> + 'a>,
        callback: Box<dyn FnOnce(&[u8]) -> Fallible<()> + 'a>,
    ) -> Fallible<()> {
        let db_env = self.db_arc.read().unwrap();
        let db = db_env
            .open_single(db_name, rkv::store::Options::create())
            .map_err(|p| format_err!("could not open db store: {:?}", p))?;

        let reader = db_env.read().expect("could not get reader");
        let cached = db
            .get(&reader, &key)
            .map_err(|p| format_err!("could not read from db: {:?}", p))?;

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
                    let mut writer = db_env.write().map_err(|p| {
                        format_err!("could not open write handle to cache: {:?}", p)
                    })?;
                    db.put(&mut writer, &key, &rkv::Value::Blob(&got))
                        .map_err(|p| format_err!("could not write to cache: {:?}", p))?;
                    writer.commit()?;
                }
            }
        };
        Ok(())
    }
}
