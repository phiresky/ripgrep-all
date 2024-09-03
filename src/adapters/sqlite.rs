use super::{writing::WritingFileAdapter, *};
use anyhow::Result;
use async_trait::async_trait;
use log::*;
use rusqlite::types::ValueRef;
use rusqlite::*;
use std::{convert::TryInto, io::Write};
use tokio::io::AsyncWrite;

use tokio_util::io::SyncIoBridge;

pub const EXTENSIONS: &[&str] = &["db", "db3", "sqlite", "sqlite3"];
pub const MIMETYPES: &[&str] = &["application/x-sqlite3"];

#[derive(Clone)]
pub struct SqliteAdapter {
    pub extensions: Vec<String>,
    pub mimetypes: Vec<String>,
}

impl Default for SqliteAdapter {
    fn default() -> SqliteAdapter {
        SqliteAdapter {
            extensions: EXTENSIONS.iter().map(|&s| s.to_string()).collect(),
            mimetypes: MIMETYPES.iter().map(|&s| s.to_string()).collect(),
        }
    }
}

impl Adapter for SqliteAdapter {
    fn name(&self) -> String {
        String::from("sqlite")
    }
    fn version(&self) -> i32 {
        1
    }
    fn description(&self) -> String {
        String::from(
            "Uses sqlite bindings to convert sqlite databases into a simple plain text format",
        )
    }
    fn recurses(&self) -> bool {
        false
    }
    fn disabled_by_default(&self) -> bool {
        false
    }
    fn keep_fast_matchers_if_accurate(&self) -> bool {
        false
    }
    fn extensions(&self) -> Vec<String> {
        self.extensions.clone()
    }
    fn mimetypes(&self) -> Vec<String> {
        self.mimetypes.clone()
    }
}

fn format_blob(b: ValueRef) -> String {
    use ValueRef::*;
    match b {
        Null => "NULL".to_owned(),
        Integer(i) => format!("{}", i),
        Real(i) => format!("{}", i),
        Text(i) => format!("'{}'", String::from_utf8_lossy(i).replace('\'', "''")),
        Blob(b) => format!(
            "[blob {}B]",
            size_format::SizeFormatterSI::new(
                // can't be larger than 2GB anyways
                b.len().try_into().unwrap()
            )
        ),
    }
}

fn synchronous_dump_sqlite(ai: AdaptInfo, mut s: impl Write) -> Result<()> {
    let AdaptInfo {
        is_real_file,
        filepath_hint,
        line_prefix,
        ..
    } = ai;
    if !is_real_file {
        // db is in an archive
        // todo: read to memory and then use that blob if size < max
        writeln!(s, "{line_prefix}[rga: skipping sqlite in archive]",)?;
        return Ok(());
    }
    let inp_fname = filepath_hint;
    let conn = Connection::open_with_flags(&inp_fname, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("opening sqlite connection to {}", inp_fname.display()))?;
    let tables: Vec<String> = conn
        .prepare("select name from sqlite_master where type='table'")
        .context("while preparing query")?
        .query_map([], |r| r.get::<_, String>(0))
        .context("while executing query")?
        .filter_map(|e| e.ok())
        .collect();
    debug!("db has {} tables", tables.len());
    for table in tables {
        // can't use query param at that position
        let mut sel = conn.prepare(&format!(
            "select * from {}",
            rusqlite::vtab::escape_double_quote(&table)
        ))?;
        let col_names: Vec<String> = sel
            .column_names()
            .into_iter()
            .map(|e| e.to_owned())
            .collect();
        let mut z = sel.query([])?;
        // writeln!(oup, "{}: {}", table, cols.join(", "))?;

        // kind of shitty (lossy) output. maybe output real csv or something?
        while let Some(row) = z.next()? {
            let row_str = col_names
                .iter()
                .enumerate()
                .map(|(i, e)| Ok(format!("{}={}", e, format_blob(row.get_ref(i)?))))
                .collect::<Result<Vec<String>>>()?
                .join(", ");
            writeln!(s, "{line_prefix}{table}: {row_str}",)?;
        }
    }
    Ok(())
}

#[async_trait]
impl WritingFileAdapter for SqliteAdapter {
    async fn adapt_write(
        ai: AdaptInfo,
        _detection_reason: &FileMatcher,
        oup: Pin<Box<dyn AsyncWrite + Send>>,
    ) -> Result<()> {
        if ai.filepath_hint.file_name().and_then(|e| e.to_str()) == Some("Thumbs.db") {
            // skip windows thumbnail cache
            return Ok(());
        }
        let oup_sync = SyncIoBridge::new(oup);
        tokio::task::spawn_blocking(|| synchronous_dump_sqlite(ai, oup_sync))
            .await?
            .context("in synchronous sqlite task")?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_utils::*;
    use pretty_assertions::assert_eq;

    #[tokio::test]
    async fn simple() -> Result<()> {
        let adapter: Box<dyn FileAdapter> = Box::<SqliteAdapter>::default();
        let fname = test_data_dir().join("hello.sqlite3");
        let (a, d) = simple_fs_adapt_info(&fname).await?;
        let res = adapter.adapt(a, &d).await?;

        let buf = adapted_to_vec(res).await?;

        assert_eq!(
            String::from_utf8(buf)?,
            "PREFIX:tbl: greeting='hello', from='sqlite database!'\nPREFIX:tbl2: x=123, y=456.789\n",
        );

        Ok(())
    }
}
