use super::*;
use crate::preproc::rga_preproc;
use ::tar::EntryType::Regular;
use failure::*;
use lazy_static::lazy_static;

use std::io::BufReader;
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

// feeling a little stupid here. why is this needed at all
enum SpecRead<R: Read> {
    Gz(flate2::read::MultiGzDecoder<R>),
    Bz2(bzip2::read::BzDecoder<R>),
    Xz(xz2::read::XzDecoder<R>),
    Zst(zstd::stream::read::Decoder<BufReader<R>>),
    Passthrough(R),
}
impl<R: Read> Read for SpecRead<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        use SpecRead::*;
        match self {
            Gz(z) => z.read(buf),
            Bz2(z) => z.read(buf),
            Xz(z) => z.read(buf),
            Zst(z) => z.read(buf),
            Passthrough(z) => z.read(buf),
        }
    }
}
// why do I need to wrap the output here in a specific type? is it possible with just a Box<Read> for every type?
fn decompress_any<'a, R>(filename: &Path, inp: &'a mut R) -> Fallible<SpecRead<&'a mut R>>
where
    R: Read,
{
    let extension = filename.extension().map(|e| e.to_string_lossy().to_owned());
    match extension {
        Some(e) => Ok(match e.to_owned().as_ref() {
            "gz" => SpecRead::Gz(flate2::read::MultiGzDecoder::new(inp)),
            "bz2" => SpecRead::Bz2(bzip2::read::BzDecoder::new(inp)),
            "xz" => SpecRead::Xz(xz2::read::XzDecoder::new_multi_decoder(inp)),
            "zst" => SpecRead::Zst(zstd::stream::read::Decoder::new(inp)?),
            "tar" => SpecRead::Passthrough(inp),
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
            ..
        } = ai;

        let decompress = decompress_any(filepath_hint, &mut inp)?;
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
                let ai2: AdaptInfo = AdaptInfo {
                    filepath_hint: &path,
                    is_real_file: false,
                    inp: &mut file,
                    oup,
                    line_prefix,
                };
                rga_preproc(ai2, None)?;
            }
        }
        Ok(())
    }
}
