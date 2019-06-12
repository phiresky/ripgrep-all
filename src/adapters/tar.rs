use super::*;
use crate::preproc::rga_preproc;
use ::tar::EntryType::Regular;
use failure::*;
use lazy_static::lazy_static;

use std::path::PathBuf;

static EXTENSIONS: &[&str] = &["tar", "tar.gz", "tar.bz2", "tar.xz", "tar.zst"];

lazy_static! {
    static ref METADATA: AdapterMeta = AdapterMeta {
        name: "tar".to_owned(),
        version: 1,
        description: "Reads a tar file as a stream and recurses down into its contents".to_owned(),
        fast_matchers: EXTENSIONS
            .iter()
            .map(|s| FastMatcher::FileExtension(s.to_string()))
            .collect(),
        slow_matchers: None
    };
}
#[derive(Default)]
pub struct TarAdapter;

impl TarAdapter {
    pub fn new() -> TarAdapter {
        TarAdapter
    }
}
impl GetMetadata for TarAdapter {
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
            "tar" => Box::new(inp),
            ext => Err(format_err!("don't know how to decompress {}", ext))?,
        }),
        None => Err(format_err!("no extension")),
    }
}

impl FileAdapter for TarAdapter {
    fn adapt(&self, ai: AdaptInfo) -> Fallible<()> {
        let AdaptInfo {
            filepath_hint,
            mut inp,
            oup,
            line_prefix,
            archive_recursion_depth,
            config,
            ..
        } = ai;

        let decompress = decompress_any(filepath_hint, &mut inp)?;
        let mut archive = ::tar::Archive::new(decompress);
        for entry in archive.entries()? {
            let mut file = entry.unwrap();
            if Regular == file.header().entry_type() {
                let path = PathBuf::from(file.path()?.to_owned());
                eprintln!(
                    "{}|{}: {} bytes",
                    filepath_hint.display(),
                    path.display(),
                    file.header().size()?,
                );
                let line_prefix = &format!("{}{}: ", line_prefix, path.display());
                let ai2: AdaptInfo = AdaptInfo {
                    filepath_hint: &path,
                    is_real_file: false,
                    archive_recursion_depth: archive_recursion_depth + 1,
                    inp: &mut file,
                    oup,
                    line_prefix,
                    config: config.clone(),
                };
                rga_preproc(ai2)?;
            }
        }
        Ok(())
    }
}
