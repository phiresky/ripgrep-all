use super::*;
use crate::preproc::rga_preproc;
use ::tar::EntryType::Regular;
use failure::*;
use lazy_static::lazy_static;
use log::*;
use std::path::PathBuf;

static EXTENSIONS: &[&str] = &["tar"];

lazy_static! {
    static ref METADATA: AdapterMeta = AdapterMeta {
        name: "tar".to_owned(),
        version: 1,
        description: "Reads a tar file as a stream and recurses down into its contents".to_owned(),
        recurses: true,
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

impl FileAdapter for TarAdapter {
    fn adapt(&self, ai: AdaptInfo, _detection_reason: &SlowMatcher) -> Fallible<()> {
        let AdaptInfo {
            filepath_hint,
            mut inp,
            oup,
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
