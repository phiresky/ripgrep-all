use super::*;
use crate::preproc::rga_preproc;
use ::zip::read::ZipFile;
use failure::*;
use lazy_static::lazy_static;

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
#[derive(Default)]
pub struct ZipAdapter;

impl ZipAdapter {
    pub fn new() -> ZipAdapter {
        ZipAdapter
    }
}
impl GetMetadata for ZipAdapter {
    fn metadata(&self) -> &AdapterMeta {
        &METADATA
    }
}

// https://github.com/mvdnes/zip-rs/commit/b9af51e654793931af39f221f143b9dea524f349
fn is_dir(f: &ZipFile) -> bool {
    f.name()
        .chars()
        .rev()
        .next()
        .map_or(false, |c| c == '/' || c == '\\')
}

impl FileAdapter for ZipAdapter {
    fn adapt(&self, ai: AdaptInfo) -> Fallible<()> {
        let AdaptInfo {
            filepath_hint,
            mut inp,
            oup,
            line_prefix,
            archive_recursion_depth,
            ..
        } = ai;
        loop {
            match ::zip::read::read_zipfile_from_stream(&mut inp) {
                Ok(None) => break,
                Ok(Some(mut file)) => {
                    if is_dir(&file) {
                        continue;
                    }
                    eprintln!(
                        "{}|{}: {} bytes ({} bytes packed)",
                        filepath_hint.to_string_lossy(),
                        file.name(),
                        file.size(),
                        file.compressed_size()
                    );
                    let line_prefix = &format!("{}{}: ", line_prefix, file.name());
                    rga_preproc(
                        AdaptInfo {
                            filepath_hint: &file.sanitized_name(),
                            is_real_file: false,
                            inp: &mut file,
                            oup,
                            line_prefix,
                            archive_recursion_depth,
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
