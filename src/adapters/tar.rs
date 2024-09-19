use crate::{adapted_iter::AdaptedFilesIterBox, matching::FileMatcher, print_bytes};
use anyhow::*;
use async_stream::stream;
use async_trait::async_trait;
use log::*;
use std::path::PathBuf;

use tokio_stream::StreamExt;

use super::{AdaptInfo, Adapter, FileAdapter};

pub const EXTENSIONS: &[&str] = &["tar"];
pub const MIMETYPES: &[&str] = &[];

#[derive(Clone)]
pub struct TarAdapter {
    pub extensions: Vec<String>,
    pub mimetypes: Vec<String>,
}

impl Default for TarAdapter {
    fn default() -> TarAdapter {
        TarAdapter {
            extensions: EXTENSIONS.iter().map(|&s| s.to_string()).collect(),
            mimetypes: MIMETYPES.iter().map(|&s| s.to_string()).collect(),
        }
    }
}

impl Adapter for TarAdapter {
    fn name(&self) -> String {
        String::from("tar")
    }
    fn version(&self) -> i32 {
        1
    }
    fn description(&self) -> String {
        String::from("Reads a tar file as a stream and recurses down into its contents")
    }
    fn recurses(&self) -> bool {
        true
    }
    fn disabled_by_default(&self) -> bool {
        false
    }
    fn keep_fast_matchers_if_accurate(&self) -> bool {
        true
    }
    fn extensions(&self) -> Vec<String> {
        self.extensions.clone()
    }
    fn mimetypes(&self) -> Vec<String> {
        self.mimetypes.clone()
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
                    let ai2: AdaptInfo = AdaptInfo {
                        filepath_hint: path,
                        is_real_file: false,
                        archive_recursion_depth: archive_recursion_depth + 1,
                        inp: Box::pin(file),
                        line_prefix: line_prefix.to_string(),
                        config: config.clone(),
                        postprocess,
                    };
                    yield Ok(ai2);
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

        let adapter = TarAdapter::default();
        let r = loop_adapt(&adapter, d, a).await.context("adapt")?;
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
