use super::*;
use lazy_static::lazy_static;
use spawning::SpawningFileAdapter;
use std::process::Command;

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

static EXTENSIONS: &[&str] = &["epub", "odt", "docx", "fb2", "ipynb"];

lazy_static! {
    static ref METADATA: AdapterMeta = AdapterMeta {
        name: "pandoc".to_owned(),
        version: 3,
        description:
            "Uses pandoc to convert binary/unreadable text documents to plain markdown-like text"
                .to_owned(),
        recurses: false,
        fast_matchers: EXTENSIONS
            .iter()
            .map(|s| FastMatcher::FileExtension(s.to_string()))
            .collect(),
        slow_matchers: None
    };
}
#[derive(Default)]
pub struct PandocAdapter;

impl PandocAdapter {
    pub fn new() -> PandocAdapter {
        PandocAdapter
    }
}
impl GetMetadata for PandocAdapter {
    fn metadata(&self) -> &AdapterMeta {
        &METADATA
    }
}
impl SpawningFileAdapter for PandocAdapter {
    fn get_exe(&self) -> &str {
        "pandoc"
    }
    fn command(&self, filepath_hint: &Path, mut cmd: Command) -> Command {
        cmd.arg("--from")
            .arg(filepath_hint.extension().unwrap())
            // simpler markown (with more information loss but plainer text)
            //.arg("--to=commonmark-header_attributes-link_attributes-fenced_divs-markdown_in_html_blocks-raw_html-native_divs-native_spans-bracketed_spans")
            .arg("--to=plain")
            .arg("--wrap=none")
            .arg("--atx-headers");
        cmd
    }
}
