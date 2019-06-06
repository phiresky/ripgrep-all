use super::*;
use crate::preproc::rga_preproc;
use failure::*;
use lazy_static::lazy_static;
use std::fs::File;
// todo:
// maybe todo: read list of extensions from
//ffmpeg -demuxers | tail -n+5 | awk '{print $2}' | while read demuxer; do echo MUX=$demuxer; ffmpeg -h demuxer=$demuxer | grep 'Common extensions'; done 2>/dev/null
static EXTENSIONS: &[&str] = &["zip"];

lazy_static! {
    static ref METADATA: AdapterMeta = AdapterMeta {
        name: "zip".to_owned(),
        version: 1,
        matchers: EXTENSIONS
            .iter()
            .map(|s| Matcher::FileExtension(s.to_string()))
            .collect(),
    };
}

pub struct ZipAdapter;

impl ZipAdapter {
    pub fn new() -> ZipAdapter {
        ZipAdapter
    }
}
impl GetMetadata for ZipAdapter {
    fn metadata<'a>(&'a self) -> &'a AdapterMeta {
        &METADATA
    }
}

impl FileAdapter for ZipAdapter {
    fn adapt(&self, ai: AdaptInfo) -> Fallible<()> {
        use std::io::prelude::*;
        let AdaptInfo {
            filepath_hint,
            mut inp,
            oup,
            line_prefix,
            ..
        } = ai;
        loop {
            match ::zip::read::read_zipfile_from_stream(&mut inp) {
                Ok(None) => break,
                Ok(Some(mut file)) => {
                    eprintln!(
                        "{}|{}: {} bytes ({} bytes packed)",
                        filepath_hint.to_string_lossy(),
                        file.name(),
                        file.size(),
                        file.compressed_size()
                    );
                    let line_prefix = &format!("{}{}: ", line_prefix, file.name().clone());
                    rga_preproc(
                        AdaptInfo {
                            filepath_hint: &file.sanitized_name(),
                            inp: &mut file,
                            oup: oup,
                            line_prefix,
                        },
                        None,
                    )?;
                }
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }
}
