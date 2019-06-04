use super::*;
use spawning::SpawningFileAdapter;

use std::io::Write;
use std::process::Command;

// from https://github.com/jgm/pandoc/blob/master/src/Text/Pandoc/App/FormatHeuristics.hs
// excluding formats that could cause problems (db = sqlite) or that are already text formats (e.g. xml-based)
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
static extensions: &[&str] = &["epub", "odt", "docx", "pptx", "fb2", "icml", "rtf", "ipynb"];

pub struct PandocAdapter {
    _metadata: AdapterMeta,
}

impl PandocAdapter {
    pub fn new() -> PandocAdapter {
        PandocAdapter {
            _metadata: AdapterMeta {
                name: "pandoc".to_owned(),
                version: 1,
                // todo: read from ffmpeg -demuxers?
                matchers: extensions.iter().map(|s| ExtensionMatcher(s)).collect(),
            },
        }
    }
}
impl GetMetadata for PandocAdapter {
    fn metadata<'a>(&'a self) -> &'a AdapterMeta {
        &self._metadata
    }
}
impl SpawningFileAdapter for PandocAdapter {
    fn command(&self, inp_fname: &str) -> Command {
        let mut cmd = Command::new("pandoc");
        cmd
            // simpler markown (with more information loss but plainer text)
            .arg("--to=markdown-header_attributes-link_attributes-fenced_divs-markdown_in_html_blocks-raw_html-native_divs-native_spans-bracketed_spans")
            .arg("--wrap=none")
            .arg("--atx-headers")
            .arg("--")
            .arg(inp_fname);
        cmd
    }
}
