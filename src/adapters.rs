pub mod ffmpeg;
pub mod pandoc;
pub mod poppler;
pub mod spawning;
use regex::{Regex, RegexSet};
use std::io::BufRead;
use std::io::Write;
use std::path::Path;
use std::rc::Rc;
use failure::*;

//pub use ffmpeg::FffmpegAdapter;

pub enum Matcher {
    // MimeType(Regex),
    FileExtension(String),
}

pub struct AdapterMeta {
    pub name: String,
    pub version: i32,
    pub matchers: Vec<Matcher>,
}

pub struct FileMeta {
    // filename is not actually a utf8 string, but since we can't do regex on OsStr and can't get a &[u8] from OsStr either,
    // and since we probably only want to do matching on ascii stuff anyways, this is the filename as a string with non-valid bytes removed
    pub lossy_filename: String,
    // pub mimetype: String,
}

pub trait GetMetadata {
    fn metadata<'a>(&'a self) -> &'a AdapterMeta;
}
pub trait FileAdapter: GetMetadata {
    fn adapt(&self, inp_fname: &Path, oup: &mut dyn Write) -> Fallible<()>;
}

pub fn extension_to_regex(extension: &str) -> Regex {
    Regex::new(&format!(".*\\.{}", &regex::escape(extension))).expect("we know this regex compiles")
}

pub fn get_adapters() -> Vec<Rc<dyn FileAdapter>> {
    let adapters: Vec<Rc<dyn FileAdapter>> = vec![
        Rc::new(crate::adapters::ffmpeg::FFmpegAdapter::new()),
        Rc::new(crate::adapters::pandoc::PandocAdapter::new()),
        Rc::new(crate::adapters::poppler::PopplerAdapter::new()),
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
