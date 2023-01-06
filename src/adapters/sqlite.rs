use crate::{adapted_iter::one_file, join_handle_to_stream, to_io_err};

use super::*;
use anyhow::Result;
use lazy_static::lazy_static;
use log::*;
use rusqlite::types::ValueRef;
use rusqlite::*;
use std::{convert::TryInto, io::Cursor};
use tokio::{
    io::AsyncReadExt,
    sync::mpsc::{self, Sender},
};
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::io::StreamReader;

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

fn yielder(ai: AdaptInfo, s: Sender<std::io::Result<Cursor<Vec<u8>>>>) -> Result<()> {
    let AdaptInfo {
        is_real_file,
        filepath_hint,
        line_prefix,
        ..
    } = ai;
    if !is_real_file {
        // db is in an archive
        // todo: read to memory and then use that blob if size < max
        s.blocking_send(Ok(Cursor::new(
            format!("{}[rga: skipping sqlite in archive]\n", line_prefix).into_bytes(),
        )))?;
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
            let str = format!(
                "{}{}: {}\n",
                line_prefix,
                table,
                col_names
                    .iter()
                    .enumerate()
                    .map(|(i, e)| Ok(format!("{}={}", e, format_blob(row.get_ref(i)?))))
                    .collect::<Result<Vec<String>>>()?
                    .join(", ")
            );
            s.blocking_send(Ok(Cursor::new(str.into_bytes())))?;
        }
    }
    Ok(())
}

impl FileAdapter for SqliteAdapter {
    fn adapt(&self, ai: AdaptInfo, _detection_reason: &FileMatcher) -> Result<AdaptedFilesIterBox> {
        let (s, r) = mpsc::channel(10);
        let filepath_hint = format!("{}.txt", ai.filepath_hint.to_string_lossy());
        let config = ai.config.clone();
        let line_prefix = ai.line_prefix.clone();
        let postprocess = ai.postprocess;
        let archive_recursion_depth = ai.archive_recursion_depth;
        let joiner = tokio::task::spawn_blocking(|| yielder(ai, s).map_err(to_io_err));
        Ok(one_file(AdaptInfo {
            is_real_file: false,
            filepath_hint: filepath_hint.into(),
            archive_recursion_depth: archive_recursion_depth + 1,
            config,
            inp: Box::pin(
                StreamReader::new(ReceiverStream::new(r)).chain(join_handle_to_stream(joiner)),
            ),
            line_prefix,
            postprocess,
        }))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_utils::*;
    use pretty_assertions::assert_eq;
    use tokio::fs::File;

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
