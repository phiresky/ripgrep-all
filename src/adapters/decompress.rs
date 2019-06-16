use super::*;
use crate::preproc::rga_preproc;
use failure::*;
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
        fast_matchers: EXTENSIONS
            .iter()
            .map(|s| FastMatcher::FileExtension(s.to_string()))
            .collect(),
        slow_matchers: Some(
            MIME_TYPES
                .iter()
                .map(|s| SlowMatcher::MimeType(s.to_string()))
                .collect()
        ),
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

fn decompress_any<'a, R>(filename: &Path, inp: &'a mut R) -> Fallible<Box<dyn Read + 'a>>
where
    R: Read,
{
    let extension = filename.extension().map(|e| e.to_string_lossy().to_owned());

    match extension {
        Some(e) => Ok(match e.to_owned().as_ref() {
            "tgz" | "gz" => Box::new(flate2::read::MultiGzDecoder::new(inp)),
            "tbz" | "tbz2" | "bz2" => Box::new(bzip2::read::BzDecoder::new(inp)),
            "xz" => Box::new(xz2::read::XzDecoder::new_multi_decoder(inp)),
            "zst" => Box::new(zstd::stream::read::Decoder::new(inp)?),
            ext => Err(format_err!("don't know how to decompress {}", ext))?,
        }),
        None => Err(format_err!("no extension")),
    }
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
    fn adapt(&self, ai: AdaptInfo, detection_reason: &SlowMatcher) -> Fallible<()> {
        let AdaptInfo {
            filepath_hint,
            mut inp,
            oup,
            line_prefix,
            archive_recursion_depth,
            config,
            ..
        } = ai;

        let mut decompress = decompress_any(filepath_hint, &mut inp)?;
        let ai2: AdaptInfo = AdaptInfo {
            filepath_hint: &get_inner_filename(filepath_hint),
            is_real_file: false,
            archive_recursion_depth: archive_recursion_depth + 1,
            inp: &mut decompress,
            oup,
            line_prefix,
            config: config.clone(),
        };
        rga_preproc(ai2)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
            assert_eq!(get_inner_filename(&PathBuf::from(a)).to_string_lossy(), *b);
        }
    }
}
