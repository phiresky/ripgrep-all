use super::*;
use lazy_static::lazy_static;
use spawning::SpawningFileAdapter;
use std::io::BufReader;
use std::process::Command;

static EXTENSIONS: &[&str] = &["pdf"];

lazy_static! {
    static ref METADATA: AdapterMeta = AdapterMeta {
        name: "poppler".to_owned(),
        version: 1,
        description: "Uses pdftotext (from poppler-utils) to extract plain text from PDF files"
            .to_owned(),
        recurses: false,
        fast_matchers: EXTENSIONS
            .iter()
            .map(|s| FastMatcher::FileExtension(s.to_string()))
            .collect(),
        slow_matchers: Some(vec![SlowMatcher::MimeType("application/pdf".to_owned())])
    };
}
#[derive(Default)]
pub struct PopplerAdapter;

impl PopplerAdapter {
    pub fn new() -> PopplerAdapter {
        PopplerAdapter
    }
}

impl GetMetadata for PopplerAdapter {
    fn metadata(&self) -> &AdapterMeta {
        &METADATA
    }
}
impl SpawningFileAdapter for PopplerAdapter {
    fn get_exe(&self) -> &str {
        "pdftotext"
    }
    fn command(&self, _filepath_hint: &Path, mut cmd: Command) -> Command {
        cmd.arg("-layout").arg("-").arg("-");
        cmd
    }
    fn postproc(line_prefix: &str, inp: &mut dyn Read, oup: &mut dyn Write) -> Fallible<()> {
        // prepend Page X to each line
        let mut page = 1;
        for line in BufReader::new(inp).lines() {
            let mut line = line?;
            if line.contains('\x0c') {
                // page break
                line = line.replace('\x0c', "");
                page += 1;
                if line.is_empty() {
                    continue;
                }
            }
            oup.write_all(format!("{}Page {}: {}\n", line_prefix, page, line).as_bytes())?;
        }
        Ok(())
    }
}
