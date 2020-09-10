use super::*;
use crate::{preproc::rga_preproc, print_bytes};
use ::tar::EntryType::Regular;
use anyhow::*;
use lazy_static::lazy_static;
use log::*;
use std::path::PathBuf;
use writing::{WritingFileAdapter, WritingFileAdapterTrait};

static EXTENSIONS: &[&str] = &["tar"];

lazy_static! {
    static ref METADATA: AdapterMeta = AdapterMeta {
        name: "tar".to_owned(),
        version: 1,
        description: "Reads a tar file as a stream and recurses down into its contents".to_owned(),
        recurses: true,
        fast_matchers: EXTENSIONS
            .iter()
            .map(|s| FastFileMatcher::FileExtension(s.to_string()))
            .collect(),
        slow_matchers: None,
        keep_fast_matchers_if_accurate: true,
        disabled_by_default: false
    };
}
#[derive(Default, Clone)]
pub struct TarAdapter;

impl TarAdapter {
    pub fn new() -> WritingFileAdapter {
        WritingFileAdapter::new(Box::new(TarAdapter))
    }
}
impl GetMetadata for TarAdapter {
    fn metadata(&self) -> &AdapterMeta {
        &METADATA
    }
}

impl WritingFileAdapterTrait for TarAdapter {
    fn adapt_write(
        &self,
        ai: AdaptInfo,
        _detection_reason: &FileMatcher,
        oup: &mut dyn Write,
    ) -> Result<()> {
        let AdaptInfo {
            filepath_hint,
            mut inp,
            line_prefix,
            archive_recursion_depth,
            config,
            ..
        } = ai;
        let mut archive = ::tar::Archive::new(&mut inp);
        for entry in archive.entries()? {
            let mut file = entry?;
            if Regular == file.header().entry_type() {
                let path = PathBuf::from(file.path()?.to_owned());
                debug!(
                    "{}|{}: {}",
                    filepath_hint.display(),
                    path.display(),
                    print_bytes(file.header().size()? as f64),
                );
                let line_prefix = &format!("{}{}: ", line_prefix, path.display());
                let ai2: AdaptInfo = AdaptInfo {
                    filepath_hint: path,
                    is_real_file: false,
                    archive_recursion_depth: archive_recursion_depth + 1,
                    inp: Box::new(file),
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
