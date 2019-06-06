pub mod ffmpeg;
pub mod pandoc;
pub mod poppler;
pub mod spawning;
pub mod tar;
pub mod zip;
use failure::*;
use regex::{Regex, RegexSet};
use std::io::prelude::*;
use std::path::Path;
use std::rc::Rc;

//pub use ffmpeg::FffmpegAdapter;

pub enum Matcher {
    // MimeType(Regex),
    /**
     * without the dot. e.g. "jpg" or "tar.gz" matched as /.*\.ext$/
     *
     */
    FileExtension(String),
}

pub struct AdapterMeta {
    pub name: String,
    pub version: i32,
    pub matchers: Vec<Matcher>,
}

pub struct FileMeta {
    // filename is not actually a utf8 string, but since we can't do regex on OsStr and can't get a &[u8] from OsStr either,
    // and since we probably only want to do only matching on ascii stuff anyways, this is the filename as a string with non-valid bytes removed
    pub lossy_filename: String,
    // pub mimetype: String,
}

pub trait GetMetadata {
    fn metadata<'a>(&'a self) -> &'a AdapterMeta;
}
pub trait FileAdapter: GetMetadata {
    fn adapt(&self, a: AdaptInfo) -> Fallible<()>;
}
pub struct AdaptInfo<'a> {
    pub filepath_hint: &'a Path,
    pub inp: &'a mut dyn Read,
    pub oup: &'a mut (dyn Write + Send),
    pub line_prefix: &'a str,
    // pub adapt_subobject: &'a dyn Fn(AdaptInfo) -> Fallible<()>,
}

pub fn extension_to_regex(extension: &str) -> Regex {
    Regex::new(&format!(".*\\.{}", &regex::escape(extension))).expect("we know this regex compiles")
}

pub fn get_adapters() -> Vec<Rc<dyn FileAdapter>> {
    let adapters: Vec<Rc<dyn FileAdapter>> = vec![
        Rc::new(ffmpeg::FFmpegAdapter::new()),
        Rc::new(pandoc::PandocAdapter::new()),
        Rc::new(poppler::PopplerAdapter::new()),
        Rc::new(zip::ZipAdapter::new()),
        Rc::new(tar::TarAdapter::new()),
    ];
    adapters
}

pub fn adapter_matcher() -> Result<impl Fn(FileMeta) -> Option<Rc<dyn FileAdapter>>, regex::Error> {
    let adapters = get_adapters();
    let mut fname_regexes = vec![];
    //let mut mime_regexes = vec![];
    for adapter in adapters.into_iter() {
        let metadata = adapter.metadata();
        for matcher in &metadata.matchers {
            match matcher {
                //Matcher::MimeType(re) => mime_regexes.push((re.clone(), adapter.clone())),
                Matcher::FileExtension(re) => {
                    fname_regexes.push((extension_to_regex(re), adapter.clone()))
                }
            };
        }
    }
    let fname_regex_set = RegexSet::new(fname_regexes.iter().map(|p| p.0.as_str()))?;
    //let mime_regex_set = RegexSet::new(mime_regexes.iter().map(|p| p.0.as_str()))?;
    return Ok(move |meta: FileMeta| {
        // todo: handle multiple conflicting matches
        for m in fname_regex_set.matches(&meta.lossy_filename) {
            return Some(fname_regexes[m].1.clone());
        }
        /*for m in mime_regex_set.matches(&meta.mimetype) {
            return Some(mime_regexes[m].1.clone());
        }*/
        return None;
    });
}
