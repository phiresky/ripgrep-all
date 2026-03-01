use crate::{adapters::FileAdapter, preproc::ActiveAdapters};
use anyhow::{Context, Result};
use log::warn;
use path_clean::PathClean;
use rusqlite::{OptionalExtension, named_params};
use std::{path::Path, time::UNIX_EPOCH};
use tokio_rusqlite::Connection;

static SCHEMA_VERSION: i32 = 3;
#[derive(Clone)]
pub struct CacheKey {
    config_hash: String,
    adapter: String,
    adapter_version: i32,
    active_adapters: String,
    file_path: String,
    file_mtime_unix_ms: i64,
}
impl CacheKey {
    pub fn new(
        postprocess: bool,
        filepath_hint: &Path,
        adapter: &dyn FileAdapter,
        active_adapters: &ActiveAdapters,
    ) -> Result<Self> {
        let meta = std::fs::metadata(filepath_hint)
            .with_context(|| format!("reading metadata for {}", filepath_hint.to_string_lossy()))?;
        let modified = meta.modified().context("could not get file modification time")?;
        let file_mtime_unix_ms = modified.duration_since(UNIX_EPOCH)?.as_millis() as i64;
        let active_adapters = if adapter.metadata().recurses {
            serde_json::to_string(
                &active_adapters
                    .iter()
                    .map(|a| format!("{}.v{}", a.metadata().name, a.metadata().version))
                    .collect::<Vec<_>>(),
            )?
        } else {
            "null".to_string()
        };
        Ok(Self {
            config_hash: if postprocess {
                "a41e2e9".to_string()
            } else {
                "f1502a3".to_string()
            }, // todo: when we add more config options that affect caching, create a struct and actually hash it
            adapter: adapter.metadata().name.clone(),
            adapter_version: adapter.metadata().version,
            file_path: filepath_hint.clean().to_string_lossy().to_string(),
            file_mtime_unix_ms,
            active_adapters,
        })
    }
}

#[async_trait::async_trait]
pub trait PreprocCache {
    async fn get(&self, key: &CacheKey) -> Result<Option<Vec<u8>>>;
    async fn set(&mut self, key: &CacheKey, value: Vec<u8>) -> Result<()>;
}

async fn connect_pragmas(db: &Connection) -> Result<()> {
    // https://phiresky.github.io/blog/2020/sqlite-performance-tuning/
    //let want_page_size = 32768;
    //db.execute(&format!("pragma page_size = {};", want_page_size))
    //    .context("setup pragma 1")?;
    db.call(|db| {
        // db.busy_timeout(Duration::from_secs(10))?;
        db.pragma_update(None, "journal_mode", "wal")?;
        db.pragma_update(None, "foreign_keys", "on")?;
        db.pragma_update(None, "temp_store", "memory")?;
        db.pragma_update(None, "synchronous", "off")?; // integrity isn't very important here
        db.pragma_update(None, "mmap_size", "2000000000")?;
        db.execute("
            create table if not exists preproc_cache (
                config_hash text not null,
                adapter text not null,
                adapter_version integer not null,
                created_unix_ms integer not null default (unixepoch() * 1000),
                active_adapters text not null, -- 'null' if adapter cannot recurse
                file_path text not null,
                file_mtime_unix_ms integer not null,
                text_content_zstd blob not null
            ) strict", []
        )?;

        db.execute("create unique index if not exists preproc_cache_idx on preproc_cache (config_hash, adapter, adapter_version, file_path, active_adapters)", [])?;

        Ok(())
    })
    .await.context("connect_pragmas")?;
    let jm: i64 = db
        .call(|db| Ok(db.pragma_query_value(None, "application_id", |r| r.get(0))?))
        .await?;
    if jm != 924716026 {
        // (probably) newly created db
        db.call(|db| Ok(db.pragma_update(None, "application_id", "924716026")?))
            .await?;
    }
    Ok(())
}

struct SqliteCache {
    db: Connection,
}
impl SqliteCache {
    async fn new(path: &Path) -> Result<Self> {
        let db = Connection::open(path.join("cache.sqlite3")).await?;
        db.call(|db| {
            let schema_version: i32 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
            if schema_version != SCHEMA_VERSION {
                warn!("Cache schema version mismatch, clearing cache");
                db.execute("drop table if exists preproc_cache", [])?;
                db.pragma_update(None, "user_version", format!("{SCHEMA_VERSION}"))?;
            }
            Ok(())
        })
        .await?;

        connect_pragmas(&db).await?;

        Ok(Self { db })
    }
}

#[async_trait::async_trait]
impl PreprocCache for SqliteCache {
    async fn get(&self, key: &CacheKey) -> Result<Option<Vec<u8>>> {
        let key = (*key).clone(); // todo: without cloning
        Ok(self
            .db
            .call(move |db| {
                Ok(db
                    .query_row(
                        "select text_content_zstd from preproc_cache where
                            adapter = :adapter
                        and config_hash = :config_hash
                        and adapter_version = :adapter_version
                        and active_adapters = :active_adapters
                        and file_path = :file_path
                        and file_mtime_unix_ms = :file_mtime_unix_ms
                ",
                        named_params! {
                            ":config_hash": &key.config_hash,
                            ":adapter": &key.adapter,
                            ":adapter_version": &key.adapter_version,
                            ":active_adapters": &key.active_adapters,
                            ":file_path": &key.file_path,
                            ":file_mtime_unix_ms": &key.file_mtime_unix_ms
                        },
                        |r| r.get::<_, Vec<u8>>(0),
                    )
                    .optional()?)
            })
            .await
            .context("reading from cache")?)
    }

    async fn set(&mut self, key: &CacheKey, value: Vec<u8>) -> Result<()> {
        let key = (*key).clone(); // todo: without cloning
        log::trace!(
            "Writing to cache: {}, {}, {} byte",
            key.adapter,
            key.file_path,
            value.len()
        );
        Ok(self
            .db
            .call(move |db| {
                db.execute(
                    "insert into preproc_cache (config_hash, adapter, adapter_version, active_adapters, file_path, file_mtime_unix_ms, text_content_zstd) values
                        (:config_hash, :adapter, :adapter_version, :active_adapters, :file_path, :file_mtime_unix_ms, :text_content_zstd)
                    on conflict (config_hash, adapter, adapter_version, active_adapters, file_path) do update set
                        file_mtime_unix_ms = :file_mtime_unix_ms,
                        created_unix_ms = unixepoch() * 1000,
                        text_content_zstd = :text_content_zstd",
                    named_params! {
                        ":config_hash": &key.config_hash,
                        ":adapter": &key.adapter,
                        ":adapter_version": &key.adapter_version,
                        ":active_adapters": &key.active_adapters,
                        ":file_path": &key.file_path,
                        ":file_mtime_unix_ms": &key.file_mtime_unix_ms,
                        ":text_content_zstd": value
                    })?;
                Ok(())
            })
            .await?)
    }
}
/// opens a default cache
pub async fn open_cache_db(path: &Path) -> Result<impl PreprocCache + use<>> {
    std::fs::create_dir_all(path)?;
    SqliteCache::new(path).await
}

#[cfg(test)]
mod test {

    use crate::preproc_cache::*;

    #[tokio::test]
    async fn test_read_write() -> anyhow::Result<()> {
        let path = tempfile::tempdir()?;
        let _db = open_cache_db(&path.path().join("foo.sqlite3")).await?;
        // db.set();
        Ok(())
    }
}
