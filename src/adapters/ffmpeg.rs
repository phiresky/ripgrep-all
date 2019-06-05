use super::*;
use spawning::SpawningFileAdapter;
use std::io::Write;
use std::process::Command;

pub struct FFmpegAdapter {
    _metadata: AdapterMeta,
}
// maybe todo: read from
// ffmpeg -demuxers
// ffmpeg -h demuxer=xyz
static extensions: &[&str] = &["mkv", "mp4", "avi"];

impl FFmpegAdapter {
    pub fn new() -> FFmpegAdapter {
        FFmpegAdapter {
            _metadata: AdapterMeta {
                name: "ffmpeg".to_owned(),
                version: 1,
                matchers: extensions.iter().map(|s| ExtensionMatcher(s)).collect(),
            },
        }
    }
}
impl GetMetadata for FFmpegAdapter {
    fn metadata<'a>(&'a self) -> &'a AdapterMeta {
        &self._metadata
    }
}
impl SpawningFileAdapter for FFmpegAdapter {
    fn command(&self, inp_fname: &str) -> Command {
        let mut cmd = Command::new("ffmpeg");
        cmd.arg("-hide_banner")
            .arg("-loglevel")
            .arg("panic")
            .arg("-i")
            .arg(inp_fname)
            .arg("-f")
            .arg("webvtt")
            .arg("-");
        cmd
    }
}
