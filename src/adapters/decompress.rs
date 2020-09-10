use super::*;
use crate::preproc::rga_preproc;
use anyhow::Result;
use lazy_static::lazy_static;

use std::path::PathBuf;

static EXTENSIONS: &[&str] = &["tgz", "tbz", "tbz2", "gz", "bz2", "xz", "zst"];
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
    pub fn new() -> DecompressAdapter {
        DecompressAdapter
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
    let gz = |inp: ReadBox| Box::new(flate2::read::MultiGzDecoder::new(inp));
    let bz2 = |inp: ReadBox| Box::new(bzip2::read::BzDecoder::new(inp));
    let xz = |inp: ReadBox| Box::new(xz2::read::XzDecoder::new_multi_decoder(inp));
    let zst = |inp: ReadBox| zstd::stream::read::Decoder::new(inp); // returns result

    Ok(match reason {
        Fast(FileExtension(ext)) => match ext.as_ref() {
            "tgz" | "gz" => gz(inp),
            "tbz" | "tbz2" | "bz2" => bz2(inp),
            "xz" => xz(inp),
            "zst" => Box::new(zst(inp)?),
            ext => Err(format_err!("don't know how to decompress {}", ext))?,
        },
        MimeType(mime) => match mime.as_ref() {
            "application/gzip" => gz(inp),
            "application/x-bzip" => bz2(inp),
            "application/x-xz" => xz(inp),
            "application/zstd" => Box::new(zst(inp)?),
            mime => Err(format_err!("don't know how to decompress mime {}", mime))?,
        },
    })
}
fn get_inner_filename(filename: &Path) -> PathBuf {
    let extension = filename
        .extension()
        .map(|e| e.to_string_lossy().to_owned())
        .unwrap_or(Cow::Borrowed(""));
    let stem = filename
        .file_stem()
        .expect("no filename given?")
        .to_string_lossy();
    let new_extension = match extension.to_owned().as_ref() {
        "tgz" | "tbz" | "tbz2" => ".tar",
        _other => "",
    };
    filename.with_file_name(format!("{}{}", stem, new_extension))
}

impl FileAdapter for DecompressAdapter {
    fn adapt(&self, ai: AdaptInfo, detection_reason: &FileMatcher) -> Result<ReadBox> {
        let AdaptInfo {
            filepath_hint,
            inp,
            line_prefix,
            archive_recursion_depth,
            config,
            ..
        } = ai;

        let ai2: AdaptInfo = AdaptInfo {
            filepath_hint: get_inner_filename(&filepath_hint),
            is_real_file: false,
            archive_recursion_depth: archive_recursion_depth + 1,
            inp: decompress_any(detection_reason, inp)?,
            line_prefix,
            config: config.clone(),
        };
        rga_preproc(ai2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::*;
    use std::fs::File;
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

    #[test]
    fn gz() -> Result<()> {
        let adapter = DecompressAdapter;

        let filepath = test_data_dir().join("hello.gz");

        let (a, d) = simple_adapt_info(&filepath, Box::new(File::open(&filepath)?));
        let mut r = adapter.adapt(a, &d)?;
        let mut o = Vec::new();
        r.read_to_end(&mut o)?;
        assert_eq!(String::from_utf8(o)?, "hello\n");
        Ok(())
    }

    #[test]
    fn pdf_gz() -> Result<()> {
        let adapter = DecompressAdapter;

        let filepath = test_data_dir().join("short.pdf.gz");

        let (a, d) = simple_adapt_info(&filepath, Box::new(File::open(&filepath)?));
        let mut r = adapter.adapt(a, &d)?;
        let mut o = Vec::new();
        r.read_to_end(&mut o)?;
        assert_eq!(
            String::from_utf8(o)?,
            "hello world
this is just a test.

1

\u{c}"
        );
        Ok(())
    }
}
