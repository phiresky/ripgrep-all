use super::*;
use spawning::SpawningFileAdapter;
use std::io::Read;
use std::io::Write;
use std::process::Command;
use std::process::Stdio;
static extensions: &[&str] = &["pdf"];

pub struct PopplerAdapter {
    _metadata: AdapterMeta,
}

impl PopplerAdapter {
    pub fn new() -> PopplerAdapter {
        PopplerAdapter {
            _metadata: AdapterMeta {
                name: "poppler pdftotext".to_owned(),
                version: 1,
                // todo: read from ffmpeg -demuxers?
                matchers: extensions.iter().map(|s| ExtensionMatcher(s)).collect(),
            },
        }
    }
}

impl GetMetadata for PopplerAdapter {
    fn metadata<'a>(&'a self) -> &'a AdapterMeta {
        &self._metadata
    }
}
impl SpawningFileAdapter for PopplerAdapter {
    fn command(&self, inp_fname: &str) -> Command {
        let mut cmd = Command::new("pdftotext");
        cmd.arg("-layout").arg("--").arg(inp_fname).arg("-");
        cmd
    }
}
