use super::spawning::map_exe_error;
use super::*;
use failure::*;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::io::BufReader;
use std::process::*;
// todo:
// maybe todo: read list of extensions from
// ffmpeg -demuxers | tail -n+5 | awk '{print $2}' | while read demuxer; do echo MUX=$demuxer; ffmpeg -h demuxer=$demuxer | grep 'Common extensions'; done 2>/dev/null
// but really, the probability of getting useful information from a .flv is low
static EXTENSIONS: &[&str] = &["mkv", "mp4", "avi"];

lazy_static! {
    static ref METADATA: AdapterMeta = AdapterMeta {
        name: "ffmpeg".to_owned(),
        version: 1,
        description: "Uses ffmpeg to extract video metadata/chapters and subtitles".to_owned(),
        recurses: false,
        fast_matchers: EXTENSIONS
            .iter()
            .map(|s| FastMatcher::FileExtension(s.to_string()))
            .collect(),
        slow_matchers: None
    };
}

#[derive(Default)]
pub struct FFmpegAdapter;

impl FFmpegAdapter {
    pub fn new() -> FFmpegAdapter {
        FFmpegAdapter
    }
}
impl GetMetadata for FFmpegAdapter {
    fn metadata(&self) -> &AdapterMeta {
        &METADATA
    }
}

#[derive(Serialize, Deserialize)]
struct FFprobeOutput {
    streams: Vec<FFprobeStream>,
}
#[derive(Serialize, Deserialize)]
struct FFprobeStream {
    codec_type: String, // video,audio,subtitle
}
impl FileAdapter for FFmpegAdapter {
    fn adapt(&self, ai: AdaptInfo, _detection_reason: &SlowMatcher) -> Fallible<()> {
        let AdaptInfo {
            is_real_file,
            filepath_hint,
            oup,
            line_prefix,
            ..
        } = ai;
        if !is_real_file {
            // we *could* probably adapt this to also work based on streams,
            // it would require using a BufReader to read at least part of the file to memory
            // but really when would you want to search for videos within archives?
            // So instead, we only run this adapter if the file is a actual file on disk for now
            writeln!(oup, "{}[rga: skipping video in archive]", line_prefix,)?;
            return Ok(());
        }
        let inp_fname = filepath_hint;
        let spawn_fail = |e| map_exe_error(e, "ffprobe", "Make sure you have ffmpeg installed.");
        let has_subtitles = {
            let probe = Command::new("ffprobe")
                .args(vec![
                    "-v",
                    "error",
                    "-select_streams",
                    "s",
                    "-of",
                    "json",
                    "-show_entries",
                    "stream=codec_type",
                ])
                .arg("-i")
                .arg(inp_fname)
                .output()
                .map_err(spawn_fail)?;
            if !probe.status.success() {
                return Err(format_err!("ffprobe failed: {:?}", probe.status));
            }
            let p: FFprobeOutput = serde_json::from_slice(&probe.stdout)?;
            (p.streams.iter().count() > 0)
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
                .arg(inp_fname)
                .stdout(Stdio::piped())
                .spawn()?;
            for line in BufReader::new(probe.stdout.as_mut().unwrap()).lines() {
                writeln!(oup, "metadata: {}", line?)?;
            }
            let exit = probe.wait()?;
            if !exit.success() {
                return Err(format_err!("ffprobe failed: {:?}", exit));
            }
        }
        if has_subtitles {
            // extract subtitles
            let mut cmd = Command::new("ffmpeg");
            cmd.arg("-hide_banner")
                .arg("-loglevel")
                .arg("panic")
                .arg("-i")
                .arg(inp_fname)
                .arg("-f")
                .arg("webvtt")
                .arg("-");
            let mut cmd = cmd.stdout(Stdio::piped()).spawn().map_err(spawn_fail)?;
            let stdo = cmd.stdout.as_mut().expect("is piped");
            let time_re = Regex::new(r".*\d.*-->.*\d.*").unwrap();
            let mut time: String = "".to_owned();
            // rewrite subtitle times so they are shown as a prefix in every line
            for line in BufReader::new(stdo).lines() {
                let line = line?;
                // 09:55.195 --> 09:56.730
                if time_re.is_match(&line) {
                    time = line.to_owned();
                } else if line.is_empty() {
                    oup.write_all(b"\n")?;
                } else {
                    writeln!(oup, "{}: {}", time, line)?;
                }
            }
        }
        Ok(())
    }
}
