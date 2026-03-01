use crate::adapted_iter::one_file;

use super::*;

use anyhow::Result;
use lazy_static::lazy_static;
use tokio::io::BufReader;

use std::path::{Path, PathBuf};

static EXTENSIONS: &[&str] = &["als", "bz2", "gz", "tbz", "tbz2", "tgz", "xz", "zst"];
static MIME_TYPES: &[&str] = &[
    "application/gzip",
    "application/x-bzip",
    "application/x-xz",
    "application/zstd",
];
lazy_static! {
    static ref METADATA: AdapterMeta = AdapterMeta {
        name: "decompress".to_owned(),
        version: 1,
        description:
            "Reads compressed file as a stream and runs a different extractor on the contents."
                .to_owned(),
        recurses: true,
        fast_matchers: EXTENSIONS
            .iter()
            .map(|s| FastFileMatcher::FileExtension(s.to_string()))
            .collect(),
        slow_matchers: Some(
            MIME_TYPES
                .iter()
                .map(|s| FileMatcher::MimeType(s.to_string()))
                .collect()
        ),
        disabled_by_default: false,
        keep_fast_matchers_if_accurate: true
    };
}
#[derive(Default)]
pub struct DecompressAdapter;

impl DecompressAdapter {
    pub fn new() -> Self {
        Self
    }
}
impl GetMetadata for DecompressAdapter {
    fn metadata(&self) -> &AdapterMeta {
        &METADATA
    }
}

fn decompress_any(reason: &FileMatcher, inp: ReadBox) -> Result<ReadBox> {
    use FastFileMatcher::*;
    use FileMatcher::*;
    use async_compression::tokio::bufread;
    let gz = |inp: ReadBox| Box::pin(bufread::GzipDecoder::new(BufReader::new(inp)));
    let bz2 = |inp: ReadBox| Box::pin(bufread::BzDecoder::new(BufReader::new(inp)));
    let xz = |inp: ReadBox| Box::pin(bufread::XzDecoder::new(BufReader::new(inp)));
    let zst = |inp: ReadBox| Box::pin(bufread::ZstdDecoder::new(BufReader::new(inp)));

    Ok(match reason {
        Fast(FileExtension(ext)) => match ext.as_ref() {
            "als" | "gz" | "tgz" => gz(inp),
            "bz2" | "tbz" | "tbz2" => bz2(inp),
            "zst" => zst(inp),
            "xz" => xz(inp),
            ext => Err(format_err!("don't know how to decompress {}", ext))?,
        },
        MimeType(mime) => match mime.as_ref() {
            "application/gzip" => gz(inp),
            "application/x-bzip" => bz2(inp),
            "application/x-xz" => xz(inp),
            "application/zstd" => zst(inp),
            mime => Err(format_err!("don't know how to decompress mime {}", mime))?,
        },
    })
}
fn get_inner_filename(filename: &Path) -> PathBuf {
    let extension = filename
        .extension()
        .map(|e| e.to_string_lossy())
        .unwrap_or(Cow::Borrowed(""));
    let stem = filename
        .file_stem()
        .expect("no filename given?")
        .to_string_lossy();
    let new_extension = match extension.as_ref() {
        "tgz" | "tbz" | "tbz2" => ".tar",
        _other => "",
    };
    filename.with_file_name(format!("{}{}", stem, new_extension))
}

#[async_trait]
impl FileAdapter for DecompressAdapter {
    async fn adapt(
        &self,
        ai: AdaptInfo,
        detection_reason: &FileMatcher,
    ) -> Result<AdaptedFilesIterBox> {
        Ok(one_file(AdaptInfo {
            filepath_hint: get_inner_filename(&ai.filepath_hint),
            is_real_file: false,
            archive_recursion_depth: ai.archive_recursion_depth + 1,
            inp: decompress_any(detection_reason, ai.inp)?,
            line_prefix: ai.line_prefix,
            config: ai.config.clone(),
            postprocess: ai.postprocess,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preproc::loop_adapt;
    use crate::test_utils::*;
    use pretty_assertions::assert_eq;
    use tokio::fs::File;

    #[test]
    fn test_inner_filename() {
        for (a, b) in &[
            ("hi/test.tgz", "hi/test.tar"),
            ("hi/hello.gz", "hi/hello"),
            ("a/b/initramfs", "a/b/initramfs"),
            ("hi/test.tbz2", "hi/test.tar"),
            ("hi/test.tbz", "hi/test.tar"),
            ("hi/test.hi.bz2", "hi/test.hi"),
            ("hello.tar.gz", "hello.tar"),
        ] {
            assert_eq!(get_inner_filename(&PathBuf::from(a)), PathBuf::from(*b));
        }
    }

    #[tokio::test]
    async fn gz() -> Result<()> {
        let adapter = DecompressAdapter;

        let filepath = test_data_dir().join("hello.gz");

        let (a, d) = simple_adapt_info(&filepath, Box::pin(File::open(&filepath).await?));
        let r = adapter.adapt(a, &d).await?;
        let o = adapted_to_vec(r).await?;
        assert_eq!(String::from_utf8(o)?, "hello\n");
        Ok(())
    }

    #[tokio::test]
    async fn pdf_gz() -> Result<()> {
        let adapter = DecompressAdapter;

        let filepath = test_data_dir().join("short.pdf.gz");

        let (a, d) = simple_adapt_info(&filepath, Box::pin(File::open(&filepath).await?));
        let engine = crate::preproc::make_engine(&a.config)?;
        let r = loop_adapt(engine, &adapter, d, a).await?;
        let o = adapted_to_vec(r).await?;
        assert_eq!(
            String::from_utf8(o)?,
            "PREFIX:Page 1: hello world
PREFIX:Page 1: this is just a test.
PREFIX:Page 1: 
PREFIX:Page 1: 1
PREFIX:Page 1: 
PREFIX:Page 1: 
"
        );
        Ok(())
    }
}
