use super::*;
use lazy_static::lazy_static;
use spawning::{SpawningFileAdapter, SpawningFileAdapterTrait};
use std::process::Command;

static EXTENSIONS: &[&str] = &["jpg", "png"];

lazy_static! {
    static ref METADATA: AdapterMeta = AdapterMeta {
        name: "tesseract".to_owned(),
        version: 1,
        description: "Uses tesseract to run OCR on images to make them searchable. May need -j1 to prevent overloading the system. Make sure you have tesseract installed.".to_owned(),
        recurses: false,
        fast_matchers: EXTENSIONS
            .iter()
            .map(|s| FastFileMatcher::FileExtension(s.to_string()))
            .collect(),
        slow_matchers: None,
        keep_fast_matchers_if_accurate: true,
        disabled_by_default: true
    };
}
#[derive(Default)]
pub struct TesseractAdapter {}

impl TesseractAdapter {
    pub fn new() -> TesseractAdapter {
        TesseractAdapter {}
    }
}

impl GetMetadata for TesseractAdapter {
    fn metadata(&self) -> &AdapterMeta {
        &METADATA
    }
}
impl SpawningFileAdapterTrait for TesseractAdapter {
    fn get_exe(&self) -> &str {
        "tesseract"
    }
    fn command(&self, _filepath_hint: &Path, mut cmd: Command) -> Command {
        // rg already does threading
        cmd.env("OMP_THREAD_LIMIT", "1").arg("-").arg("-");
        Some(cmd)
    }
}
