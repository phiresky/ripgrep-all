use crate::{adapters::FileAdapter, preproc::ActiveAdapters};
use anyhow::{Context, Result};
use path_clean::PathClean;
use rusqlite::{named_params, OptionalExtension};
use std::{path::Path, time::UNIX_EPOCH};
use tokio_rusqlite::Connection;

#[derive(Clone)]
pub struct CacheKey {
    adapter: String,
    adapter_version: i32,
    active_adapters: String,
    file_path: String,
    file_mtime_unix_ms: i64,
}
impl CacheKey {
    pub fn new(
        filepath_hint: &Path,
        adapter: &dyn FileAdapter,
        active_adapters: &ActiveAdapters,
    ) -> Result<CacheKey> {
        let meta = std::fs::metadata(filepath_hint)
            .with_context(|| format!("reading metadata for {}", filepath_hint.to_string_lossy()))?;
        let modified = meta.modified().expect("weird OS that can't into mtime");
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
        Ok(CacheKey {
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
        db.execute_batch(
            "
    pragma journal_mode = WAL;
    pragma foreign_keys = on;
    pragma temp_store = memory;
    pragma synchronous = off; -- integrity isn't very important here
    pragma mmap_size = 30000000000;

    create table if not exists preproc_cache (
        adapter text not null,
        adapter_version integer not null,
        created_unix_ms integer not null default (unixepoch() * 1000),
        active_adapters text not null, -- 'null' if adapter cannot recurse
        file_path text not null,
        file_mtime_unix_ms integer not null,
        text_content_zstd blob not null
    ) strict;
    
    create unique index if not exists preproc_cache_idx on preproc_cache (adapter, adapter_version, file_path, active_adapters);
    ",
        )
    })
    .await.context("connect_pragmas")?;
    let jm: i64 = db
        .call(|db| db.pragma_query_value(None, "application_id", |r| r.get(0)))
        .await?;
    if jm != 924716026 {
        // (probably) newly created db
        create_pragmas(db).await.context("create_pragmas")?;
    }
    Ok(())
}

async fn create_pragmas(db: &Connection) -> Result<()> {
    db.call(|db| {
        db.execute_batch(
            "
        pragma application_id = 924716026;
        pragma user_version = 2; -- todo: on upgrade clear db if version is unexpected
        ",
        )
    })
    .await?;
    Ok(())
}
struct SqliteCache {
    db: Connection,
}
impl SqliteCache {
    async fn new(path: &Path) -> Result<SqliteCache> {
        let db = Connection::open(path.join("cache.sqlite3")).await?;
        connect_pragmas(&db).await?;

        Ok(SqliteCache { db })
    }
}

#[async_trait::async_trait]
impl PreprocCache for SqliteCache {
    async fn get(&self, key: &CacheKey) -> Result<Option<Vec<u8>>> {
        let key = (*key).clone(); // todo: without cloning
        Ok(self
            .db
            .call(move |db| {
                db.query_row(
                    "select text_content_zstd from preproc_cache where
                            adapter = :adapter
                        and adapter_version = :adapter_version
                        and active_adapters = :active_adapters
                        and file_path = :file_path
                        and file_mtime_unix_ms = :file_mtime_unix_ms
                ",
                    named_params! {
                        ":adapter": &key.adapter,
                        ":adapter_version": &key.adapter_version,
                        ":active_adapters": &key.active_adapters,
                        ":file_path": &key.file_path,
                        ":file_mtime_unix_ms": &key.file_mtime_unix_ms
                    },
                    |r| r.get::<_, Vec<u8>>(0),
                )
                .optional()
            })
            .await
            .context("reading from cache")?)
    }

    async fn set(&mut self, key: &CacheKey, value: Vec<u8>) -> Result<()> {
        let key = (*key).clone(); // todo: without cloning
        Ok(self
            .db
            .call(move |db| {
                db.execute(
                    "insert into preproc_cache (adapter, adapter_version, active_adapters, file_path, file_mtime_unix_ms, text_content_zstd) values
                        (:adapter, :adapter_version, :active_adapters, :file_path, :file_mtime_unix_ms, :text_content_zstd)
                    on conflict (adapter, adapter_version, active_adapters, file_path) do update set
                        file_mtime_unix_ms = :file_mtime_unix_ms,
                        created_unix_ms = unixepoch() * 1000,
                        text_content_zstd = :text_content_zstd",
                    named_params! {
                        ":adapter": &key.adapter,
                        ":adapter_version": &key.adapter_version,
                        ":active_adapters": &key.active_adapters,
                        ":file_path": &key.file_path,
                        ":file_mtime_unix_ms": &key.file_mtime_unix_ms,
                        ":text_content_zstd": value
                    }
                ).map(|_| ())
            })
            .await?)
    }
}
/// opens a default cache
pub async fn open_cache_db(path: &Path) -> Result<impl PreprocCache> {
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
