use super::{writing::WritingFileAdapter, *};
use anyhow::Result;
use async_trait::async_trait;
use lazy_static::lazy_static;
use serde_json::Value;
use std::io::Write;
use tokio::io::{AsyncReadExt, AsyncWrite};
use tokio_util::io::SyncIoBridge;

static EXTENSIONS: &[&str] = &["ipynb"];

lazy_static! {
    static ref METADATA: AdapterMeta = AdapterMeta {
        name: "ipynb".to_owned(),
        version: 1,
        description: "Flattens Jupyter notebooks: markdown + code, optional outputs".to_owned(),
        recurses: false,
        fast_matchers: EXTENSIONS
            .iter()
            .map(|s| FastFileMatcher::FileExtension(s.to_string()))
            .collect(),
        slow_matchers: Some(vec![FileMatcher::MimeType(
            "application/x-ipynb+json".to_owned(),
        )]),
        keep_fast_matchers_if_accurate: false,
        disabled_by_default: false,
    };
}

#[derive(Default, Clone)]
pub struct IpynbAdapter;

impl IpynbAdapter {
    pub fn new() -> Self {
        Self
    }
}
impl GetMetadata for IpynbAdapter {
    fn metadata(&self) -> &AdapterMeta {
        &METADATA
    }
}

fn write_lines(value: &Value, s: &mut impl Write) -> Result<()> {
    match value {
        Value::Array(arr) => {
            for v in arr {
                match v {
                    Value::String(st) => {
                        writeln!(s, "{}", st)?;
                    }
                    _ => {
                        writeln!(s, "{}", v)?;
                    }
                }
            }
        }
        Value::String(st) => {
            writeln!(s, "{}", st)?;
        }
        _ => {}
    }
    Ok(())
}

fn write_cell_sources(cell: &Value, s: &mut impl Write) -> Result<()> {
    if let Some(src) = cell.get("source") {
        write_lines(src, s)?;
    }
    Ok(())
}

fn write_output(o: &Value, s: &mut impl Write) -> Result<()> {
    if let Some(t) = o.get("text") {
        write_lines(t, s)?;
    }
    if let Some(d) = o.get("data") {
        if let Some(tp) = d.get("text/plain") {
            write_lines(tp, s)?;
        } else if let Some(th) = d.get("text/html") {
            write_lines(th, s)?;
        } else if let Some(ta) = d.get("application/json") {
            writeln!(s, "{}", ta)?;
        }
    }
    if let Some(ot) = o.get("output_type").and_then(|x| x.as_str())
        && ot == "error"
    {
        if let Some(tb) = o.get("traceback") {
            write_lines(tb, s)?;
        } else {
            let en = o.get("ename").and_then(|x| x.as_str()).unwrap_or("");
            let ev = o.get("evalue").and_then(|x| x.as_str()).unwrap_or("");
            if !en.is_empty() || !ev.is_empty() {
                writeln!(s, "{} {}", en, ev)?;
            }
        }
    }
    Ok(())
}

fn flatten_ipynb(bytes: &[u8], include_outputs: bool, mut s: impl Write) -> Result<()> {
    let v: Value = serde_json::from_slice(bytes)?;
    if let Some(cells) = v.get("cells").and_then(|c| c.as_array()) {
        for cell in cells {
            let ct = cell.get("cell_type").and_then(|x| x.as_str()).unwrap_or("");
            if ct == "markdown" || ct == "raw" {
                write_cell_sources(cell, &mut s)?;
            } else if ct == "code" {
                write_cell_sources(cell, &mut s)?;
                if include_outputs
                    && let Some(outs) = cell.get("outputs").and_then(|o| o.as_array())
                {
                    for o in outs {
                        write_output(o, &mut s)?;
                    }
                }
            }
        }
    }
    Ok(())
}

#[async_trait]
impl WritingFileAdapter for IpynbAdapter {
    async fn adapt_write(
        mut ai: AdaptInfo,
        _detection_reason: &FileMatcher,
        oup: Pin<Box<dyn AsyncWrite + Send>>,
    ) -> Result<()> {
        let mut buf = Vec::new();
        ai.inp.read_to_end(&mut buf).await?;
        let include_outputs = ai.config.ipynb_include_outputs;
        let oup_sync = SyncIoBridge::new(oup);
        tokio::task::spawn_blocking(move || flatten_ipynb(&buf, include_outputs, oup_sync))
            .await??;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_utils::*;
    use pretty_assertions::assert_eq;
    use std::io::Cursor;

    fn sample_notebook(include_outputs: bool) -> (AdaptInfo, FileMatcher) {
        let json = r##"{
            "cells": [
                {"cell_type": "markdown", "source": ["# Title\n", "Intro text\n"]},
                {"cell_type": "code", "source": ["print(\"hello\")\n"], "outputs": [
                    {"output_type": "stream", "name": "stdout", "text": ["hello\n"]}
                ]}
            ],
            "metadata": {}, "nbformat": 4, "nbformat_minor": 5
        }"##;
        let mut cfg = RgaConfig::default();
        cfg.ipynb_include_outputs = include_outputs;
        let ai = AdaptInfo {
            filepath_hint: PathBuf::from("nb.ipynb"),
            is_real_file: false,
            archive_recursion_depth: 0,
            inp: Box::pin(Cursor::new(json.as_bytes().to_vec())),
            line_prefix: "PREFIX:".to_string(),
            config: cfg,
            postprocess: false,
        };
        (ai, FastFileMatcher::FileExtension("ipynb".to_string()).into())
    }

    #[tokio::test]
    async fn flatten_without_outputs() -> Result<()> {
        let adapter: Box<dyn FileAdapter> = Box::<IpynbAdapter>::default();
        let (ai, det) = sample_notebook(false);
        let res = adapter.adapt(ai, &det).await?;
        let buf = adapted_to_vec(res).await?;
        assert_eq!(String::from_utf8(buf)?, "PREFIX:# Title\nPREFIX:Intro text\nPREFIX:print(\"hello\")\n");
        Ok(())
    }

    #[tokio::test]
    async fn flatten_with_outputs() -> Result<()> {
        let adapter: Box<dyn FileAdapter> = Box::<IpynbAdapter>::default();
        let (ai, det) = sample_notebook(true);
        let res = adapter.adapt(ai, &det).await?;
        let buf = adapted_to_vec(res).await?;
        assert_eq!(String::from_utf8(buf)?, "PREFIX:# Title\nPREFIX:Intro text\nPREFIX:print(\"hello\")\nPREFIX:hello\n");
        Ok(())
    }
}