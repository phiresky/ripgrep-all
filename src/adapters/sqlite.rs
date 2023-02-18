use super::{writing::WritingFileAdapter, *};
use anyhow::Result;
use async_trait::async_trait;
use lazy_static::lazy_static;
use log::*;
use rusqlite::types::ValueRef;
use rusqlite::*;
use std::{convert::TryInto, io::Write};
use tokio::io::AsyncWrite;

use tokio_util::io::SyncIoBridge;

static EXTENSIONS: &[&str] = &["db", "db3", "sqlite", "sqlite3"];

lazy_static! {
    static ref METADATA: AdapterMeta = AdapterMeta {
        name: "sqlite".to_owned(),
        version: 1,
        description:
            "Uses sqlite bindings to convert sqlite databases into a simple plain text format"
                .to_owned(),
        recurses: false, // set to true if we decide to make sqlite blobs searchable (gz blob in db is kinda common I think)
        fast_matchers: EXTENSIONS
            .iter()
            .map(|s| FastFileMatcher::FileExtension(s.to_string()))
            .collect(),
        slow_matchers: Some(vec![FileMatcher::MimeType(
            "application/x-sqlite3".to_owned()
        )]),
        keep_fast_matchers_if_accurate: false,
        disabled_by_default: false
    };
}

#[derive(Default, Clone)]
pub struct SqliteAdapter;

impl SqliteAdapter {
    pub fn new() -> SqliteAdapter {
        SqliteAdapter
    }
}
impl GetMetadata for SqliteAdapter {
    fn metadata(&self) -> &AdapterMeta {
        &METADATA
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

    let conn = Connection::open_with_flags(inp_fname, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let tables: Vec<String> = conn
        .prepare("select name from sqlite_master where type='table'")?
        .query_map([], |r| r.get::<_, String>(0))?
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
        let oup_sync = SyncIoBridge::new(oup);
        tokio::task::spawn_blocking(|| synchronous_dump_sqlite(ai, oup_sync)).await??;
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
        let adapter: Box<dyn FileAdapter> = Box::new(SqliteAdapter::default());
        let fname = test_data_dir().join("hello.sqlite3");
        let (a, d) = simple_fs_adapt_info(&fname).await?;
        let res = adapter.adapt(a, &d)?;

        let buf = adapted_to_vec(res).await?;

        assert_eq!(
            String::from_utf8(buf)?,
            "PREFIX:tbl: greeting='hello', from='sqlite database!'\nPREFIX:tbl2: x=123, y=456.789\n",
        );

        Ok(())
    }
}
