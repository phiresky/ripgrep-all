use super::*;
use crate::adapters::spawning::map_exe_error;
use crate::preproc::rga_preproc;
use lazy_static::lazy_static;
use log::*;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::process::Command;

static EXTENSIONS: &[&str] = &["pdf"];

lazy_static! {
    static ref METADATA: AdapterMeta = AdapterMeta {
        name: "pdfpages".to_owned(),
        version: 1,
        description: "Converts a pdf to its individual pages as png files. Only useful in combination with tesseract".to_owned(),
        recurses: true,
        fast_matchers: EXTENSIONS
            .iter()
            .map(|s| FastMatcher::FileExtension(s.to_string()))
            .collect(),
        slow_matchers: Some(vec![SlowMatcher::MimeType(
            "application/pdf".to_owned()
        )])
    };
}
#[derive(Default)]
pub struct PdfPagesAdapter {}

impl PdfPagesAdapter {
    pub fn new() -> PdfPagesAdapter {
        PdfPagesAdapter {}
    }
}

impl GetMetadata for PdfPagesAdapter {
    fn metadata(&self) -> &AdapterMeta {
        &METADATA
    }
}

/// A pdf is basically converted to a zip that has Page X.png files.
/// This way, something like tesseract can process the pages individually
impl FileAdapter for PdfPagesAdapter {
    fn adapt(&self, ai: AdaptInfo, _detection_reason: &SlowMatcher) -> Fallible<()> {
        let AdaptInfo {
            filepath_hint,
            is_real_file,
            oup,
            line_prefix,
            archive_recursion_depth,
            config,
            ..
        } = ai;
        if !is_real_file {
            // todo: read to memory and then use that blob if size < max
            writeln!(oup, "{}[rga: skipping pdfpages in archive]", line_prefix,)?;
            return Ok(());
        }
        let inp_fname = filepath_hint;
        let exe_name = "gm";
        let out_dir = tempfile::Builder::new().prefix("pdfpages-").tempdir()?;
        let out_fname = out_dir.path().join("out%04d.png");
        debug!("writing to temp dir: {}", out_fname.display());
        let mut cmd = Command::new(exe_name);
        cmd.arg("convert")
            .arg("-density")
            .arg("200")
            .arg(inp_fname)
            .arg("+adjoin")
            .arg(out_fname);

        let mut cmd = cmd
            .spawn()
            .map_err(|e| map_exe_error(e, exe_name, "Make sure you have imagemagick installed."))?;
        let args = config.args;

        let status = cmd.wait()?;
        if status.success() {
        } else {
            return Err(format_err!("subprocess failed: {:?}", status));
        }
        for (i, filename) in glob::glob(
            out_dir
                .path()
                .join("out*.png")
                .to_str()
                .expect("temp path has invalid encoding"),
        )?
        .enumerate()
        {
            let mut ele = BufReader::new(File::open(filename?)?);
            rga_preproc(AdaptInfo {
                filepath_hint: &PathBuf::from(format!("Page {}.png", i + 1)),
                is_real_file: false,
                inp: &mut ele,
                oup,
                line_prefix: &format!("{}Page {}:", line_prefix, i + 1),
                archive_recursion_depth: archive_recursion_depth + 1,
                config: PreprocConfig { cache: None, args },
            })?;
        }
        Ok(())
    }
}

/*// todo: do this in an actually streaming fashion and less slow
// IEND chunk + PDF magic
// 4945 4e44 ae42 6082 8950 4e47 0d0a 1a0a
let split_seq = hex_literal::hex!("4945 4e44 ae42 6082 8950 4e47 0d0a 1a0a");
let split_seq_inx = 8;
fn split_by_seq<'a>(
    split_seq: &'a [u8],
    split_inx: usize,
    read: &mut Read,
) -> Fallible<impl IntoIterator<Item = impl Read> + 'a> {
    let regex = split_seq
        .iter()
        .map(|c| format!("\\x{:0>2x}", c))
        .collect::<Vec<_>>()
        .join("");
    let restr = format!("(?-u){}", regex);
    eprintln!("re: {}", restr);
    let re = regex::bytes::Regex::new(&restr)?;

    let mut all = Vec::new();
    read.read_to_end(&mut all)?;
    let mut out: Vec<Cursor<Vec<u8>>> = Vec::new();
    let mut last = 0;
    for (i, split) in re.find_iter(&all).enumerate() {
        let pos = split.start() + split_inx;
        out.push(Cursor::new(Vec::from(&all[last..pos])));
        last = pos;
    }
    out.push(Cursor::new(Vec::from(&all[last..])));
    Ok(out)
}*/
