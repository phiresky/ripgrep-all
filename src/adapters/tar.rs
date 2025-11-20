use crate::{
    adapted_iter::AdaptedFilesIterBox,
    adapters::AdapterMeta,
    matching::{FastFileMatcher, FileMatcher},
    print_bytes,
};
use anyhow::*;
use async_stream::stream;
use async_trait::async_trait;
use lazy_static::lazy_static;
use log::*;
use std::path::PathBuf;

use tokio_stream::StreamExt;
use tokio::sync::Semaphore;
use tokio::io::{duplex, copy};
use tokio::sync::mpsc;
use std::time::Instant;
use std::sync::Mutex;

use super::{AdaptInfo, FileAdapter, GetMetadata};

static EXTENSIONS: &[&str] = &["tar"];

lazy_static! {
    static ref METADATA: AdapterMeta = AdapterMeta {
        name: "tar".to_owned(),
        version: 1,
        description: "Reads a tar file as a stream and recurses down into its contents".to_owned(),
        recurses: true,
        fast_matchers: EXTENSIONS
            .iter()
            .map(|s| FastFileMatcher::FileExtension(s.to_string()))
            .collect(),
        slow_matchers: None,
        keep_fast_matchers_if_accurate: true,
        disabled_by_default: false
    };
}
lazy_static! { static ref TAR_PIPE_TUNED: Mutex<Option<usize>> = Mutex::new(None); }
fn get_tar_pipe_bytes(cfg: &crate::config::RgaConfig) -> usize {
    let mut g = TAR_PIPE_TUNED.lock().unwrap();
    if let Some(v) = *g { v } else { let v = cfg.tar_pipe_bytes.0; *g = Some(v); v }
}
fn set_tar_pipe_bytes(new: usize) { let mut g = TAR_PIPE_TUNED.lock().unwrap(); *g = Some(new); }
#[derive(Default, Clone)]
pub struct TarAdapter;

impl TarAdapter {
    pub fn new() -> Self {
        Self
    }
}
impl GetMetadata for TarAdapter {
    fn metadata(&self) -> &AdapterMeta {
        &METADATA
    }
}

#[async_trait]
impl FileAdapter for TarAdapter {
    async fn adapt(
        &self,
        ai: AdaptInfo,
        _detection_reason: &FileMatcher,
    ) -> Result<AdaptedFilesIterBox> {
        let AdaptInfo {
            filepath_hint,
            inp,
            line_prefix,
            archive_recursion_depth,
            config,
            postprocess,
            ..
        } = ai;
        let mut archive = ::tokio_tar::Archive::new(inp);
        let mut entries = archive.entries()?;
        let sem = std::sync::Arc::new(Semaphore::new(config.tar_max_concurrency.0));
        let (tx, mut rx) = mpsc::unbounded_channel::<(usize, f64)>();
        {
            let min_cap: usize = 1<<18; // 256 KiB
            let max_cap: usize = 1<<20; // 1 MiB
            let cfg = config.clone();
            tokio::spawn(async move {
                let mut samples: Vec<(usize, f64)> = Vec::new();
                let mut cur = get_tar_pipe_bytes(&cfg);
                while let Some(s) = rx.recv().await {
                    samples.push(s);
                    if samples.len() >= 8 {
                        let mut sum = 0.0f64;
                        for (_, secs) in samples.iter() { sum += *secs; }
                        let avg = sum / (samples.len() as f64);
                        let mut new = cur;
                        if avg > 0.2 { new = (cur.saturating_mul(2)).min(max_cap); }
                        else if avg < 0.05 { new = (cur.saturating_div(2)).max(min_cap); }
                        if new != cur { set_tar_pipe_bytes(new); cur = new; }
                        samples.clear();
                    }
                }
            });
        }
        let s = stream! {
            while let Some(entry) = entries.next().await {
                let file = entry?;
                if tokio_tar::EntryType::Regular == file.header().entry_type() {
                    let path = PathBuf::from(file.path()?.to_owned());
                    debug!(
                        "{}|{}: {}",
                        filepath_hint.display(),
                        path.display(),
                        print_bytes(file.header().size().unwrap_or(0) as f64),
                    );
                    let line_prefix = &format!("{}{}: ", line_prefix, path.display());
                    let (w, r) = duplex(get_tar_pipe_bytes(&config));
                    let sem2 = sem.clone();
                    let tx2 = tx.clone();
                    tokio::spawn(async move {
                        let _p = sem2.acquire_owned().await.unwrap();
                        let mut src = file;
                        let mut dst = w;
                        let start = Instant::now();
                        let n = copy(&mut src, &mut dst).await.unwrap_or(0);
                        let secs = (Instant::now() - start).as_secs_f64();
                        let _ = tx2.send((n as usize, secs));
                    });
                    yield Ok(AdaptInfo {
                        filepath_hint: path,
                        is_real_file: false,
                        archive_recursion_depth: archive_recursion_depth + 1,
                        inp: Box::pin(r),
                        line_prefix: line_prefix.to_string(),
                        config: config.clone(),
                        postprocess,
                    });
                }
            }
        };

        Ok(Box::pin(s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{preproc::loop_adapt, test_utils::*};
    use pretty_assertions::assert_eq;
    use tokio::fs::File;

    #[tokio::test]
    async fn test_simple_tar() -> Result<()> {
        let filepath = test_data_dir().join("hello.tar");

        let (a, d) = simple_adapt_info(&filepath, Box::pin(File::open(&filepath).await?));

        let adapter = TarAdapter::new();
        let engine = crate::preproc::make_engine(&a.config)?;
        let r = loop_adapt(engine, &adapter, d, a).await.context("adapt")?;
        let o = adapted_to_vec(r).await.context("adapted_to_vec")?;
        assert_eq!(
            String::from_utf8(o).context("parsing utf8")?,
            "PREFIX:dir/file-b.pdf: Page 1: hello world
PREFIX:dir/file-b.pdf: Page 1: this is just a test.
PREFIX:dir/file-b.pdf: Page 1: 
PREFIX:dir/file-b.pdf: Page 1: 1
PREFIX:dir/file-b.pdf: Page 1: 
PREFIX:dir/file-b.pdf: Page 1: 
PREFIX:dir/file-a.pdf: Page 1: hello world
PREFIX:dir/file-a.pdf: Page 1: this is just a test.
PREFIX:dir/file-a.pdf: Page 1: 
PREFIX:dir/file-a.pdf: Page 1: 1
PREFIX:dir/file-a.pdf: Page 1: 
PREFIX:dir/file-a.pdf: Page 1: 
"
        );
        Ok(())
    }
}
