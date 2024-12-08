use super::*;
use super::{AdaptInfo, Adapter, FileAdapter};
use crate::adapted_iter::one_file;

use crate::{adapted_iter::AdaptedFilesIterBox, expand::expand_str_ez, matching::FileMatcher};
use crate::{join_handle_to_stream, to_io_err};
use anyhow::Result;
use async_stream::stream;
use bytes::Bytes;
use lazy_static::lazy_static;
use log::debug;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Stdio;
use tokio::io::AsyncReadExt;
use tokio::process::Child;
use tokio::process::Command;

use tokio_util::io::StreamReader;
// mostly the same as AdapterMeta + SpawningFileAdapter

#[derive(Debug, Deserialize, Serialize, JsonSchema, Default, PartialEq, Clone)]
pub struct CustomIdentifier {
    /// The file extensions this adapter supports, for example `["gz", "tgz"]`.
    pub extensions: Option<Vec<String>>,
    /// If not null and --rga-accurate is enabled, mimetype matching is used instead of file name matching.
    pub mimetypes: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, PartialEq, Clone)]
pub struct CustomIdentifiers {
    /// The identifiers to process as bz2 archives.
    pub bz2: Option<CustomIdentifier>,
    /// The identifiers to process as gz archives.
    pub gz: Option<CustomIdentifier>,
    /// The identifiers to process as xz archives.
    pub xz: Option<CustomIdentifier>,
    /// The identifiers to process as zst archives.
    pub zst: Option<CustomIdentifier>,

    /// The identifiers to process via ffmpeg.
    pub ffmpeg: Option<CustomIdentifier>,

    /// The identifiers to process as mbox files.
    pub mbox: Option<CustomIdentifier>,

    /// The identifiers to process as SQLite files.
    pub sqlite: Option<CustomIdentifier>,

    /// The identifiers to process as tar files.
    pub tar: Option<CustomIdentifier>,

    /// The identifiers to process as zip archives.
    pub zip: Option<CustomIdentifier>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Default, PartialEq, Clone)]
pub struct CustomAdapterConfig {
    /// The unique identifier and name of this adapter.
    ///
    /// Must only include a-z, 0-9, _.
    pub name: String,

    /// The description of this adapter shown in help.
    pub description: String,

    /// If true, the adapter will be disabled by default.
    pub disabled_by_default: Option<bool>,

    /// Version identifier used to key cache entries.
    ///
    /// Change this if the configuration or program changes.
    pub version: i32,

    /// The file extensions this adapter supports, for example `["epub", "mobi"]`.
    pub extensions: Vec<String>,

    /// If not empty `--rga-accurate` is enabled, mimetype matching is used instead of file name matching.
    pub mimetypes: Vec<String>,

    /// If `--rga-accurate`, only match by mime types and ignore extensions completely.
    pub match_only_by_mime: Option<bool>,

    /// The name or path of the binary to run.
    pub binary: String,

    /// The arguments to run the program with.
    /// Placeholders:
    /// - `$input_file_extension`: the file extension (without dot). e.g. foo.tar.gz -> gz
    /// - `$input_file_stem`: the file name without the last extension. e.g. foo.tar.gz -> foo.tar
    /// - `$input_virtual_path`: the full input file path.
    ///   Note that this path may not actually exist on disk because it is the result of another adapter.
    ///
    /// stdin of the program will be connected to the input file, and stdout is assumed to be the converted file
    pub args: Vec<String>,

    /// The output path hint.
    /// The placeholders are the same as for `.args`
    ///
    /// If not set, defaults to `"${input_virtual_path}.txt"`.
    ///
    /// Setting this is useful if the output format is not plain text (.txt) but instead some other format that should be passed to another adapter
    pub output_path_hint: Option<String>,
}

pub fn strs(arr: &[&str]) -> Vec<String> {
    arr.iter().map(ToString::to_string).collect()
}

lazy_static! {
    pub static ref BUILTIN_SPAWNING_ADAPTERS: Vec<CustomAdapterConfig> = vec![
        // from https://github.com/jgm/pandoc/blob/master/src/Text/Pandoc/App/FormatHeuristics.hs
        // excluding formats that could cause problems (.db ?= sqlite) or that are already text formats (e.g. xml-based)
        //"db"       -> Just "docbook"
        //"adoc"     -> Just "asciidoc"
        //"asciidoc" -> Just "asciidoc"
        //"context"  -> Just "context"
        //"ctx"      -> Just "context"
        //"dokuwiki" -> Just "dokuwiki"
        //"htm"      -> Just "html"
        //"html"     -> Just "html"
        //"json"     -> Just "json"
        //"latex"    -> Just "latex"
        //"lhs"      -> Just "markdown+lhs"
        //"ltx"      -> Just "latex"
        //"markdown" -> Just "markdown"
        //"md"       -> Just "markdown"
        //"ms"       -> Just "ms"
        //"muse"     -> Just "muse"
        //"native"   -> Just "native"
        //"opml"     -> Just "opml"
        //"org"      -> Just "org"
        //"roff"     -> Just "ms"
        //"rst"      -> Just "rst"
        //"s5"       -> Just "s5"
        //"t2t"      -> Just "t2t"
        //"tei"      -> Just "tei"
        //"tei.xml"  -> Just "tei"
        //"tex"      -> Just "latex"
        //"texi"     -> Just "texinfo"
        //"texinfo"  -> Just "texinfo"
        //"textile"  -> Just "textile"
        //"text"     -> Just "markdown"
        //"txt"      -> Just "markdown"
        //"xhtml"    -> Just "html"
        //"wiki"     -> Just "mediawiki"
        CustomAdapterConfig {
            name: "pandoc".to_string(),
            description: "Uses pandoc to convert binary/unreadable text documents to plain markdown-like text".to_string(),
            version: 3,
            extensions: strs(&["epub", "odt", "docx", "fb2", "ipynb", "html", "htm"]),
            binary: "pandoc".to_string(),
            mimetypes: Vec::new(),
            // simpler markdown (with more information loss but plainer text)
            //.arg("--to=commonmark-header_attributes-link_attributes-fenced_divs-markdown_in_html_blocks-raw_html-native_divs-native_spans-bracketed_spans")
            args: strs(&[
                "--from=$input_file_extension",
                "--to=plain",
                "--wrap=none",
                "--markdown-headings=atx"
            ]),
            disabled_by_default: None,
            match_only_by_mime: None,
            output_path_hint: None
        },
        CustomAdapterConfig {
            name: "poppler".to_owned(),
            version: 1,
            description: "Uses pdftotext (from poppler-utils) to extract plain text from PDF files"
                .to_owned(),

            extensions: strs(&["pdf"]),
            mimetypes: strs(&["application/pdf"]),
            binary: "pdftotext".to_string(),
            args: strs(&["-", "-"]),
            disabled_by_default: None,
            match_only_by_mime: None,
            output_path_hint: Some("${input_virtual_path}.txt.asciipagebreaks".into())
        }
    ];
}

/// replace a Command.spawn() error "File not found" with a more readable error
/// to indicate some program is not installed
pub fn map_exe_error(err: std::io::Error, exe_name: &str, help: &str) -> anyhow::Error {
    use std::io::ErrorKind::*;
    match err.kind() {
        NotFound => format_err!("Could not find executable \"{}\". {}", exe_name, help),
        _ => anyhow::Error::from(err),
    }
}

fn proc_wait(mut child: Child, context: impl FnOnce() -> String) -> impl AsyncRead {
    let s = stream! {
        let res = child.wait().await?;
        if res.success() {
            yield std::io::Result::Ok(Bytes::new());
        } else {
            Err(format_err!("{:?}", res)).with_context(context).map_err(to_io_err)?;
        }
    };
    StreamReader::new(s)
}

pub fn pipe_output(
    _line_prefix: &str,
    mut cmd: Command,
    inp: ReadBox,
    exe_name: &str,
    help: &str,
) -> Result<ReadBox> {
    let cmd_log = format!("{:?}", cmd); // todo: perf
    let mut cmd = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| map_exe_error(e, exe_name, help))?;
    let mut stdi = cmd.stdin.take().expect("is piped");
    let stdo = cmd.stdout.take().expect("is piped");

    let join = tokio::spawn(async move {
        let mut z = inp;
        tokio::io::copy(&mut z, &mut stdi).await?;
        std::io::Result::Ok(())
    });
    Ok(Box::pin(stdo.chain(
        proc_wait(cmd, move || format!("subprocess: {cmd_log}")).chain(join_handle_to_stream(join)),
    )))
}

pub struct CustomSpawningFileAdapter {
    name: String,
    version: i32,
    description: String,
    recurses: bool,
    disabled_by_default: bool,
    keep_fast_matchers_if_accurate: bool,
    extensions: Vec<String>,
    mimetypes: Vec<String>,
    binary: String,
    args: Vec<String>,
    output_path_hint: Option<String>,
}

impl Adapter for CustomSpawningFileAdapter {
    fn name(&self) -> String {
        self.name.clone()
    }
    fn version(&self) -> i32 {
        self.version
    }
    fn description(&self) -> String {
        self.description.clone()
    }
    fn recurses(&self) -> bool {
        self.recurses
    }
    fn disabled_by_default(&self) -> bool {
        self.disabled_by_default
    }
    fn keep_fast_matchers_if_accurate(&self) -> bool {
        self.keep_fast_matchers_if_accurate
    }
    fn extensions(&self) -> Vec<String> {
        self.extensions.clone()
    }
    fn mimetypes(&self) -> Vec<String> {
        self.mimetypes.clone()
    }
}

fn arg_replacer(arg: &str, filepath_hint: &Path) -> Result<String> {
    expand_str_ez(arg, |s| match s {
        "input_virtual_path" => Ok(filepath_hint.to_string_lossy()),
        "input_file_stem" => Ok(filepath_hint
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()),
        "input_file_extension" => Ok(filepath_hint
            .extension()
            .unwrap_or_default()
            .to_string_lossy()),
        e => Err(anyhow::format_err!("unknown replacer ${{{e}}}")),
    })
}
impl CustomSpawningFileAdapter {
    fn command(
        &self,
        filepath_hint: &std::path::Path,
        mut command: tokio::process::Command,
    ) -> Result<tokio::process::Command> {
        command.args(
            self.args
                .iter()
                .map(|arg| arg_replacer(arg, filepath_hint))
                .collect::<Result<Vec<_>>>()?,
        );
        log::debug!("running command {:?}", command);
        Ok(command)
    }
}
#[async_trait]
impl FileAdapter for CustomSpawningFileAdapter {
    async fn adapt(
        &self,
        ai: AdaptInfo,
        _detection_reason: &FileMatcher,
    ) -> Result<AdaptedFilesIterBox> {
        let AdaptInfo {
            filepath_hint,
            inp,
            line_prefix,
            archive_recursion_depth,
            postprocess,
            config,
            ..
        } = ai;

        let cmd = Command::new(&self.binary);
        let cmd = self
            .command(&filepath_hint, cmd)
            .with_context(|| format!("Could not set cmd arguments for {}", self.binary))?;
        debug!("executing {:?}", cmd);
        let output = pipe_output(&line_prefix, cmd, inp, &self.binary, "")?;
        Ok(one_file(AdaptInfo {
            filepath_hint: PathBuf::from(arg_replacer(
                self.output_path_hint
                    .as_deref()
                    .unwrap_or("${input_virtual_path}.txt"),
                &filepath_hint,
            )?),
            inp: output,
            line_prefix,
            is_real_file: false,
            archive_recursion_depth: archive_recursion_depth + 1,
            postprocess,
            config,
        }))
    }
}
impl CustomAdapterConfig {
    pub fn to_adapter(&self) -> CustomSpawningFileAdapter {
        CustomSpawningFileAdapter {
            name: self.name.clone(),
            version: self.version,
            description: format!(
                "{}\nRuns: {} {}",
                self.description,
                self.binary,
                self.args.join(" ")
            ),
            recurses: false,
            disabled_by_default: self.disabled_by_default.unwrap_or(false),
            keep_fast_matchers_if_accurate: !self.match_only_by_mime.unwrap_or(false),
            extensions: self.extensions.clone(),
            mimetypes: self.mimetypes.clone(),
            binary: self.binary.clone(),
            args: self.args.clone(),
            output_path_hint: self.output_path_hint.clone(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::super::FileAdapter;
    use super::*;
    use crate::preproc::loop_adapt;
    use crate::test_utils::*;
    use anyhow::Result;
    use pretty_assertions::assert_eq;
    use tokio::fs::File;

    #[tokio::test]
    async fn poppler() -> Result<()> {
        let adapter = poppler_adapter();

        let filepath = test_data_dir().join("short.pdf");

        let (a, d) = simple_adapt_info(&filepath, Box::pin(File::open(&filepath).await?));
        // let r = adapter.adapt(a, &d)?;
        let r = loop_adapt(&adapter, d, a).await?;
        let o = adapted_to_vec(r).await?;
        assert_eq!(
            String::from_utf8(o)?,
            ["hello world", "this is just a test.", "", "1", "", "\n",]
                .map(|s| format!("{}{}", "PREFIX:Page 1: ", s))
                .join("\n")
        );
        Ok(())
    }

    use crate::{
        adapters::custom::CustomAdapterConfig,
        test_utils::{adapted_to_vec, simple_adapt_info},
    };
    use std::io::Cursor;

    #[tokio::test]
    async fn streaming() -> anyhow::Result<()> {
        // an adapter that converts input line by line (deadlocks if the parent process tries to write everything and only then read it)
        let adapter = CustomAdapterConfig {
            name: "simple text replacer".to_string(),
            description: "oo".to_string(),
            disabled_by_default: None,
            version: 1,
            extensions: vec!["txt".to_string()],
            mimetypes: Vec::new(),
            match_only_by_mime: None,
            binary: "sed".to_string(),
            args: vec!["s/e/u/g".to_string()],
            output_path_hint: None,
        };

        let adapter = adapter.to_adapter();
        let input = r#"
        This is the story of a
        very strange lorry
        with a long dead crew
        and a witch with the flu
        "#;
        let input = format!("{input}{input}{input}{input}");
        let input = format!("{input}{input}{input}{input}");
        let input = format!("{input}{input}{input}{input}");
        let input = format!("{input}{input}{input}{input}");
        let input = format!("{input}{input}{input}{input}");
        let input = format!("{input}{input}{input}{input}");
        let (a, d) = simple_adapt_info(
            Path::new("foo.txt"),
            Box::pin(Cursor::new(Vec::from(input))),
        );
        let output = adapter.adapt(a, &d).await.unwrap();

        let oup = adapted_to_vec(output).await?;
        println!("output: {}", String::from_utf8_lossy(&oup));
        Ok(())
    }
}
