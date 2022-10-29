use super::{
    spawning::{SpawningFileAdapter, SpawningFileAdapterTrait},
    AdapterMeta, GetMetadata,
};
use crate::matching::{FastFileMatcher, FileMatcher};
use anyhow::Result;
use lazy_static::lazy_static;
use regex::{Captures, Regex};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;

// mostly the same as AdapterMeta + SpawningFileAdapter
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default, PartialEq, Clone)]
pub struct CustomAdapterConfig {
    /// the unique identifier and name of this adapter. Must only include a-z, 0-9, _
    pub name: String,
    /// a description of this adapter. shown in help
    pub description: String,
    /// if true, the adapter will be disabled by default
    pub disabled_by_default: Option<bool>,
    /// version identifier. used to key cache entries, change if the configuration or program changes
    pub version: i32,
    /// the file extensions this adapter supports. For example ["epub", "mobi"]
    pub extensions: Vec<String>,
    /// if not null and --rga-accurate is enabled, mime type matching is used instead of file name matching
    pub mimetypes: Option<Vec<String>>,
    /// if --rga-accurate, only match by mime types, ignore extensions completely
    pub match_only_by_mime: Option<bool>,
    /// the name or path of the binary to run
    pub binary: String,
    /// The arguments to run the program with. Placeholders:
    /// {}: the file path (TODO)
    /// stdin of the program will be connected to the input file, and stdout is assumed to be the converted file
    pub args: Vec<String>,
    // TODO: make adapter filename configurable (?) for inner matching (e.g. foo.tar.gz should be foo.tar after gunzipping)
}

fn strs(arr: &[&str]) -> Vec<String> {
    arr.iter().map(ToString::to_string).collect()
}

lazy_static! {
    pub static ref builtin_spawning_adapters: Vec<CustomAdapterConfig> = vec![
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
            extensions: strs(&["epub", "odt", "docx", "fb2", "ipynb"]),
            binary: "pandoc".to_string(),
            mimetypes: None,
            // simpler markown (with more information loss but plainer text)
            //.arg("--to=commonmark-header_attributes-link_attributes-fenced_divs-markdown_in_html_blocks-raw_html-native_divs-native_spans-bracketed_spans")
            args: strs(&[
                "--from={file_extension}",
                "--to=plain",
                "--wrap=none",
                "--markdown-headings=atx"
            ]),
            disabled_by_default: None,
            match_only_by_mime: None
        },
        CustomAdapterConfig {
            name: "poppler".to_owned(),
            version: 1,
            description: "Uses pdftotext (from poppler-utils) to extract plain text from PDF files"
                .to_owned(),

            extensions: strs(&["pdf"]),
            mimetypes: Some(strs(&["application/pdf"])),

            binary: "pdftotext".to_string(),
            args: strs(&["-", "-"]),
            disabled_by_default: None,
            match_only_by_mime: None
            // postprocessors: [{name: "add_page_numbers_by_pagebreaks"}]
        }
    ];
}

pub struct CustomSpawningFileAdapter {
    binary: String,
    args: Vec<String>,
    meta: AdapterMeta,
}
impl GetMetadata for CustomSpawningFileAdapter {
    fn metadata(&self) -> &AdapterMeta {
        &self.meta
    }
}
fn arg_replacer(arg: &str, filepath_hint: &Path) -> Result<String> {
    lazy_static::lazy_static! {
        static ref ARG_REP: Regex = Regex::new(r"\{([a-z_]+)\}").unwrap();
    }
    let mut err = None;
    let r = ARG_REP.replace_all(arg, |m: &Captures| -> String {
        let idx = m.get(0).unwrap().range();
        if arg.chars().nth(idx.start - 1) == Some('{') {
            // skip
            return m.get(0).unwrap().as_str().to_string();
        }
        if arg.chars().nth(idx.end + 1) == Some('}') {
            // skip
            return m.get(0).unwrap().as_str().to_string();
        }
        let key = m.get(1).unwrap().as_str();
        if key == "file_extension" {
            return filepath_hint
                .extension()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or("".to_string());
        }
        err = Some(anyhow::anyhow!(
            "Unknown arg replacement key '{}' in '{}'",
            key,
            arg
        ));
        "".to_string()
        //let
    });
    if let Some(err) = err {
        Err(err)
    } else {
        Ok(r.to_string())
    }
}
impl SpawningFileAdapterTrait for CustomSpawningFileAdapter {
    fn get_exe(&self) -> &str {
        &self.binary
    }
    fn command(
        &self,
        filepath_hint: &std::path::Path,
        mut command: tokio::process::Command,
    ) -> Result<tokio::process::Command> {
        command.args(
            self.args
                .iter()
                .map(|arg| arg_replacer(arg, &filepath_hint))
                .collect::<Result<Vec<_>>>()?,
        );
        log::debug!("running command {:?}", command);
        Ok(command)
    }
}
impl CustomAdapterConfig {
    pub fn to_adapter(&self) -> SpawningFileAdapter {
        let ad = CustomSpawningFileAdapter {
            binary: self.binary.clone(),
            args: self.args.clone(),
            meta: AdapterMeta {
                name: self.name.clone(),
                version: self.version,
                description: format!(
                    "{}\nRuns: {} {}",
                    self.description,
                    self.binary,
                    self.args.join(" ")
                ),
                recurses: false,
                fast_matchers: self
                    .extensions
                    .iter()
                    .map(|s| FastFileMatcher::FileExtension(s.to_string()))
                    .collect(),
                slow_matchers: self.mimetypes.as_ref().map(|mimetypes| {
                    mimetypes
                        .iter()
                        .map(|s| FileMatcher::MimeType(s.to_string()))
                        .collect()
                }),
                keep_fast_matchers_if_accurate: !self.match_only_by_mime.unwrap_or(false),
                disabled_by_default: self.disabled_by_default.unwrap_or(false),
            },
        };
        SpawningFileAdapter::new(Box::new(ad))
    }
}

#[cfg(test)]
mod test {
    use super::super::FileAdapter;
    use super::*;
    use crate::test_utils::*;
    use anyhow::Result;
    use tokio::fs::File;

    #[tokio::test]
    async fn poppler() -> Result<()> {
        let adapter = builtin_spawning_adapters
            .iter()
            .find(|e| e.name == "poppler")
            .expect("no poppler adapter");

        let adapter = adapter.to_adapter();

        let filepath = test_data_dir().join("short.pdf");

        let (a, d) = simple_adapt_info(&filepath, Box::pin(File::open(&filepath).await?));
        let r = adapter.adapt(a, &d)?;
        let o = adapted_to_vec(r)?;
        assert_eq!(
            String::from_utf8(o)?,
            "PREFIX:hello world
PREFIX:this is just a test.
PREFIX:
PREFIX:1
PREFIX:
PREFIX:\u{c}
"
        );
        Ok(())
    }
}
