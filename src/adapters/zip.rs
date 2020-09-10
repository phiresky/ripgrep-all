use super::*;
use crate::{preproc::rga_preproc, print_bytes};
use ::zip::read::ZipFile;
use anyhow::*;
use lazy_static::lazy_static;
use log::*;
use writing::{WritingFileAdapter, WritingFileAdapterTrait};

// todo:
// maybe todo: read list of extensions from
//ffmpeg -demuxers | tail -n+5 | awk '{print $2}' | while read demuxer; do echo MUX=$demuxer; ffmpeg -h demuxer=$demuxer | grep 'Common extensions'; done 2>/dev/null
static EXTENSIONS: &[&str] = &["zip"];

lazy_static! {
    static ref METADATA: AdapterMeta = AdapterMeta {
        name: "zip".to_owned(),
        version: 1,
        description: "Reads a zip file as a stream and recurses down into its contents".to_owned(),
        recurses: true,
        fast_matchers: EXTENSIONS
            .iter()
            .map(|s| FastFileMatcher::FileExtension(s.to_string()))
            .collect(),
        slow_matchers: Some(vec![FileMatcher::MimeType("application/zip".to_owned())]),
        keep_fast_matchers_if_accurate: false,
        disabled_by_default: false
    };
}
#[derive(Default, Clone)]
pub struct ZipAdapter;

impl ZipAdapter {
    pub fn new() -> WritingFileAdapter {
        WritingFileAdapter::new(Box::new(ZipAdapter))
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

impl WritingFileAdapterTrait for ZipAdapter {
    fn adapt_write<'a>(
        &self,
        ai: AdaptInfo<'a>,
        _detection_reason: &FileMatcher,
        oup: &mut (dyn Write + 'a),
    ) -> Result<()> {
        let AdaptInfo {
            filepath_hint,
            mut inp,
            line_prefix,
            archive_recursion_depth,
            config,
            ..
        } = ai;
        loop {
            match ::zip::read::read_zipfile_from_stream(&mut inp) {
                Ok(None) => break,
                Ok(Some(mut file)) => {
                    if is_dir(&file) {
                        continue;
                    }
                    debug!(
                        "{}{}|{}: {} ({} packed)",
                        line_prefix,
                        filepath_hint.to_string_lossy(),
                        file.name(),
                        print_bytes(file.size() as f64),
                        print_bytes(file.compressed_size() as f64)
                    );
                    let line_prefix = format!("{}{}: ", line_prefix, file.name());
                    let mut rd = rga_preproc(AdaptInfo {
                        filepath_hint: file.sanitized_name().clone(),
                        is_real_file: false,
                        inp: &mut file,
                        line_prefix,
                        archive_recursion_depth: archive_recursion_depth + 1,
                        config: config.clone(),
                    })?;
                    // copy read stream from inner file to output
                    std::io::copy(&mut rd, oup);
                    drop(rd);
                }
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }
}
