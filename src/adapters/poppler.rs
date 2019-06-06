use super::*;
use lazy_static::lazy_static;
use spawning::SpawningFileAdapter;
use std::process::Command;

static EXTENSIONS: &[&str] = &["pdf"];

lazy_static! {
    static ref METADATA: AdapterMeta = AdapterMeta {
        name: "poppler".to_owned(),
        version: 1,
        matchers: EXTENSIONS
            .iter()
            .map(|s| Matcher::FileExtension(s.to_string()))
            .collect(),
    };
}
pub struct PopplerAdapter;

impl PopplerAdapter {
    pub fn new() -> PopplerAdapter {
        PopplerAdapter
    }
}

impl GetMetadata for PopplerAdapter {
    fn metadata<'a>(&'a self) -> &'a AdapterMeta {
        &METADATA
    }
}
impl SpawningFileAdapter for PopplerAdapter {
    fn get_exe(&self) -> &str {
        "pdftotext"
    }
    fn command(&self, filepath_hint: &Path, mut cmd: Command) -> Command {
        cmd.arg("-layout").arg("-").arg("-");
        cmd
    }
}
