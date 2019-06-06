use super::*;
use crate::preproc::rga_preproc;
use ::tar::EntryType::Regular;
use failure::*;
use lazy_static::lazy_static;
use std::fs::File;
use std::path::PathBuf;

static EXTENSIONS: &[&str] = &["tar", "tar.gz", "tar.bz2", "tar.xz", "tar.zst"];

lazy_static! {
    static ref METADATA: AdapterMeta = AdapterMeta {
        name: "tar".to_owned(),
        version: 1,
        matchers: EXTENSIONS
            .iter()
            .map(|s| Matcher::FileExtension(s.to_string()))
            .collect(),
    };
}

pub struct TarAdapter;

impl TarAdapter {
    pub fn new() -> TarAdapter {
        TarAdapter
    }
}
impl GetMetadata for TarAdapter {
    fn metadata<'a>(&'a self) -> &'a AdapterMeta {
        &METADATA
    }
}
/*struct WrapRead<'a> {
    inner: &mut 'a Read;
}
impl Read for WrapRead {
    r
}*/

/*fn decompress_any(filename: &Path, inp: &mut Read) -> Fallible<Box<Read>> {
    let extension = filename.extension().map(|e| e.to_string_lossy().to_owned());
    match extension {
        Some(e) => Ok(match e.to_owned().as_ref() {
            "gz" => Box::new(flate2::read::MultiGzDecoder::new(inp)),
            "bz2" => Box::new(bzip2::read::BzDecoder::new(inp)),
            "xz" => Box::new(xz2::read::XzDecoder::new_multi_decoder(inp)),
            "zst" => Box::new(zstd::stream::read::Decoder::new(inp)?),
            e => Err(format_err!("don't know how to decompress {}", e))?,
        }),
        None => Err(format_err!("no extension")),
    }
}*/

impl FileAdapter for TarAdapter {
    fn adapt<'a>(&self, ai: AdaptInfo) -> Fallible<()> {
        use std::io::prelude::*;
        let AdaptInfo {
            filepath_hint,
            mut inp,
            oup,
            line_prefix,
            ..
        } = ai;
        let decompress = inp; //decompress_any(filepath_hint, &inp)?;
        let mut archive = ::tar::Archive::new(decompress);
        for entry in archive.entries()? {
            let mut file = entry.unwrap();
            let path = PathBuf::from(file.path()?.to_owned());
            eprintln!(
                "{}|{}: {} bytes",
                filepath_hint.display(),
                path.display(),
                file.header().size()?,
            );
            if Regular == file.header().entry_type() {
                let line_prefix = &format!("{}{}: ", line_prefix, path.display());
                rga_preproc(
                    AdaptInfo {
                        filepath_hint: &path,
                        inp: &mut file,
                        oup: oup,
                        line_prefix,
                    },
                    None,
                )?;
            }
        }
        Ok(())
    }
}
