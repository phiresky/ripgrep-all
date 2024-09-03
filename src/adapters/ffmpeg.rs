use super::*;
use super::{custom::map_exe_error, writing::async_writeln};
use anyhow::*;
use async_trait::async_trait;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::process::Stdio;
use tokio::io::AsyncWrite;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use writing::WritingFileAdapter;

// maybe todo: read list of extensions from
// ffmpeg -demuxers | tail -n+5 | awk '{print $2}' | while read demuxer; do echo MUX=$demuxer; ffmpeg -h demuxer=$demuxer | grep 'Common extensions'; done 2>/dev/null
// but really, the probability of getting useful information from a .flv is low
pub const EXTENSIONS: &[&str] = &["mkv", "mp4", "avi", "mp3", "ogg", "flac", "webm"];
pub const MIMETYPES: &[&str] = &[];

#[derive(Clone)]
pub struct FFmpegAdapter {
    pub extensions: Vec<String>,
    pub mimetypes: Vec<String>,
}

impl Adapter for FFmpegAdapter {
    fn name(&self) -> String {
        String::from("ffmpeg")
    }
    fn version(&self) -> i32 {
        1
    }
    fn description(&self) -> String {
        String::from(
            "Uses ffmpeg to extract video metadata/chapters, subtitles, lyrics, and other metadata.",
        )
    }
    fn recurses(&self) -> bool {
        false
    }
    fn disabled_by_default(&self) -> bool {
        false
    }
    fn keep_fast_matchers_if_accurate(&self) -> bool {
        true
    }
    fn extensions(&self) -> Vec<String> {
        self.extensions.clone()
    }
    fn mimetypes(&self) -> Vec<String> {
        self.mimetypes.clone()
    }
}

#[derive(Serialize, Deserialize)]
struct FFprobeOutput {
    streams: Vec<FFprobeStream>,
}
#[derive(Serialize, Deserialize)]
struct FFprobeStream {
    index: i32, // stream index
}

#[async_trait]
impl WritingFileAdapter for FFmpegAdapter {
    async fn adapt_write(
        ai: AdaptInfo,
        _detection_reason: &FileMatcher,
        mut oup: Pin<Box<dyn AsyncWrite + Send>>,
    ) -> Result<()> {
        let AdaptInfo {
            is_real_file,
            filepath_hint,
            line_prefix,
            ..
        } = ai;
        if !is_real_file {
            // we *could* probably adapt this to also work based on streams,
            // it would require using a BufReader to read at least part of the file to memory
            // but really when would you want to search for videos within archives?
            // So instead, we only run this adapter if the file is a actual file on disk for now
            async_writeln!(oup, "{line_prefix}[rga: skipping video in archive]\n")?;
            return Ok(());
        }
        let inp_fname = filepath_hint;
        let spawn_fail = |e| map_exe_error(e, "ffprobe", "Make sure you have ffmpeg installed.");
        let subtitle_streams = {
            let probe = Command::new("ffprobe")
                .args(vec![
                    "-v",
                    "error", // show all errors
                    "-select_streams",
                    "s", // show only subtitle streams
                    "-of",
                    "json", // use json as output format
                    "-show_entries",
                    "stream=index", // show index of subtitle streams
                ])
                .arg("-i")
                .arg(&inp_fname)
                .output()
                .await
                .map_err(spawn_fail)?;
            if !probe.status.success() {
                return Err(format_err!(
                    "ffprobe failed: {:?}\n{}",
                    probe.status,
                    String::from_utf8_lossy(&probe.stderr)
                ));
            }
            let p: FFprobeOutput = serde_json::from_slice(&probe.stdout)?;
            p.streams
        };
        {
            // extract file metadata (especially chapter names in a greppable format)
            let mut probe = Command::new("ffprobe")
                .args(vec![
                    "-v",
                    "error",
                    "-show_format",
                    "-show_streams",
                    "-of",
                    "flat",
                    // "-show_data",
                    "-show_error",
                    "-show_programs",
                    "-show_chapters",
                    // "-count_frames",
                    //"-count_packets",
                ])
                .arg("-i")
                .arg(&inp_fname)
                .stdout(Stdio::piped())
                .spawn()?;
            let mut lines = BufReader::new(probe.stdout.as_mut().unwrap()).lines();
            while let Some(line) = lines.next_line().await? {
                let line = line.replace("\\r\\n", "\n").replace("\\n", "\n"); // just unescape newlines
                async_writeln!(oup, "metadata: {line}")?;
            }
            let exit = probe.wait().await?;
            if !exit.success() {
                return Err(format_err!("ffprobe failed: {:?}", exit));
            }
        }
        if !subtitle_streams.is_empty() {
            for probe_stream in subtitle_streams.iter() {
                // extract subtitles
                let mut cmd = Command::new("ffmpeg");
                cmd.arg("-hide_banner")
                    .arg("-loglevel")
                    .arg("panic")
                    .arg("-i")
                    .arg(&inp_fname)
                    .arg("-map")
                    .arg(format!("0:{}", probe_stream.index)) // 0 for first input
                    .arg("-f")
                    .arg("webvtt")
                    .arg("-");
                let mut cmd = cmd.stdout(Stdio::piped()).spawn().map_err(spawn_fail)?;
                let stdo = cmd.stdout.as_mut().expect("is piped");
                let time_re = Regex::new(r".*\d.*-->.*\d.*").unwrap();
                let mut time: String = "".to_owned();
                // rewrite subtitle times so they are shown as a prefix in every line
                let mut lines = BufReader::new(stdo).lines();
                while let Some(line) = lines.next_line().await? {
                    // 09:55.195 --> 09:56.730
                    if time_re.is_match(&line) {
                        time = line.to_owned();
                    } else if line.is_empty() {
                        async_writeln!(oup)?;
                    } else {
                        async_writeln!(oup, "{time}: {line}")?;
                    }
                }
            }
        }
        Ok(())
    }
}
