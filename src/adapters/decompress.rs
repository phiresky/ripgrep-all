use crate::adapted_iter::one_file;

use super::*;

use anyhow::Result;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use tokio::io::BufReader;

pub const EXTENSIONS_GZ: &[&str] = &["als", "gz", "tgz"];
pub const EXTENSIONS_BZ2: &[&str] = &["bz2", "tbz", "tbz2"];
pub const EXTENSIONS_XZ: &[&str] = &["xz"];
pub const EXTENSIONS_ZST: &[&str] = &["zst"];

#[derive(Debug, PartialEq, Eq)]
struct DecompressError;

#[derive(Debug, PartialEq)]
enum Extension {
    Gz,
    Bz2,
    Xz,
    Zst,
}
impl FromStr for Extension {
    type Err = DecompressError;

    fn from_str(ext: &str) -> Result<Self, Self::Err> {
        if EXTENSIONS_GZ.contains(&ext) {
            Ok(Extension::Gz)
        } else if EXTENSIONS_BZ2.contains(&ext) {
            Ok(Extension::Bz2)
        } else if EXTENSIONS_XZ.contains(&ext) {
            Ok(Extension::Xz)
        } else if EXTENSIONS_ZST.contains(&ext) {
            Ok(Extension::Zst)
        } else {
            Err(DecompressError)
        }
    }
}

pub const MIMETYPES_GZ: &[&str] = &["application/gzip"];
pub const MIMETYPES_BZ2: &[&str] = &["application/x-bzip"];
pub const MIMETYPES_XZ: &[&str] = &["application/x-xz"];
pub const MIMETYPES_ZST: &[&str] = &["application/zstd"];

#[derive(Debug, PartialEq)]
enum Mime {
    Gz,
    Bz2,
    Xz,
    Zst,
}
impl FromStr for Mime {
    type Err = DecompressError;

    fn from_str(ext: &str) -> Result<Self, Self::Err> {
        if MIMETYPES_GZ.contains(&ext) {
            Ok(Mime::Gz)
        } else if MIMETYPES_BZ2.contains(&ext) {
            Ok(Mime::Bz2)
        } else if MIMETYPES_XZ.contains(&ext) {
            Ok(Mime::Xz)
        } else if MIMETYPES_ZST.contains(&ext) {
            Ok(Mime::Zst)
        } else {
            Err(DecompressError)
        }
    }
}

#[derive(Default)]
pub struct DecompressAdapter {
    pub extensions_gz: Vec<String>,
    pub extensions_bz2: Vec<String>,
    pub extensions_xz: Vec<String>,
    pub extensions_zst: Vec<String>,
    pub mimetypes_gz: Vec<String>,
    pub mimetypes_bz2: Vec<String>,
    pub mimetypes_xz: Vec<String>,
    pub mimetypes_zst: Vec<String>,
}

impl Adapter for DecompressAdapter {
    fn name(&self) -> String {
        String::from("decompress")
    }
    fn version(&self) -> i32 {
        1
    }
    fn description(&self) -> String {
        String::from(
            "Reads compressed file as a stream and runs a different extractor on the contents.",
        )
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
        let mut extensions: Vec<String> = Vec::new();
        for exts in [
            &self.extensions_gz,
            &self.extensions_bz2,
            &self.extensions_xz,
            &self.extensions_zst,
        ] {
            for ext in exts {
                extensions.push(ext.to_string())
            }
        }
        extensions
    }
    fn mimetypes(&self) -> Vec<String> {
        let mut mimetypes: Vec<String> = Vec::new();
        for mimes in [
            &self.mimetypes_gz,
            &self.mimetypes_bz2,
            &self.mimetypes_xz,
            &self.mimetypes_zst,
        ] {
            for mime in mimes {
                mimetypes.push(mime.to_string())
            }
        }
        mimetypes
    }
}

fn decompress_any(reason: &FileMatcher, inp: ReadBox) -> Result<ReadBox> {
    use async_compression::tokio::bufread;
    use FastFileMatcher::*;
    use FileMatcher::*;
    let gz = |inp: ReadBox| Box::pin(bufread::GzipDecoder::new(BufReader::new(inp)));
    let bz2 = |inp: ReadBox| Box::pin(bufread::BzDecoder::new(BufReader::new(inp)));
    let xz = |inp: ReadBox| Box::pin(bufread::XzDecoder::new(BufReader::new(inp)));
    let zst = |inp: ReadBox| Box::pin(bufread::ZstdDecoder::new(BufReader::new(inp)));

    Ok(match reason {
        Fast(FileExtension(ext)) => match Extension::from_str(ext) {
            Ok(Extension::Gz) => gz(inp),
            Ok(Extension::Bz2) => bz2(inp),
            Ok(Extension::Zst) => xz(inp),
            Ok(Extension::Xz) => zst(inp),
            Err(_) => Err(format_err!("don't know how to decompress {}", ext))?,
        },
        MimeType(mime) => match Mime::from_str(mime) {
            Ok(Mime::Gz) => gz(inp),
            Ok(Mime::Bz2) => bz2(inp),
            Ok(Mime::Xz) => xz(inp),
            Ok(Mime::Zst) => zst(inp),
            Err(_) => Err(format_err!("don't know how to decompress mime {}", mime))?,
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
        let adapter = DecompressAdapter::default();

        let filepath = test_data_dir().join("hello.gz");

        let (a, d) = simple_adapt_info(&filepath, Box::pin(File::open(&filepath).await?));
        let r = adapter.adapt(a, &d).await?;
        let o = adapted_to_vec(r).await?;
        assert_eq!(String::from_utf8(o)?, "hello\n");
        Ok(())
    }

    #[tokio::test]
    async fn pdf_gz() -> Result<()> {
        let adapter = DecompressAdapter::default();

        let filepath = test_data_dir().join("short.pdf.gz");

        let (a, d) = simple_adapt_info(&filepath, Box::pin(File::open(&filepath).await?));
        let r = loop_adapt(&adapter, d, a).await?;
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
