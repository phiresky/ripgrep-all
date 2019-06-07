pub mod ffmpeg;
pub mod pandoc;
pub mod poppler;
pub mod spawning;
pub mod sqlite;
pub mod tar;
pub mod zip;
use crate::preproc::PreprocConfig;
use failure::*;
use log::*;
use regex::{Regex, RegexSet};
use std::collections::HashMap;
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
    /// unique short name of this adapter (a-z0-9 only)
    pub name: String,
    /// version identifier. used to key cache entries, change if your output format changes
    pub version: i32,
    pub description: String,
    pub matchers: Vec<Matcher>,
}

pub struct FileMeta {
    // filename is not actually a utf8 string, but since we can't do regex on OsStr and can't get a &[u8] from OsStr either,
    // and since we probably only want to do only matching on ascii stuff anyways, this is the filename as a string with non-valid bytes removed
    pub lossy_filename: String,
    // pub mimetype: String,
}

pub trait GetMetadata {
    fn metadata(&self) -> &AdapterMeta;
}
pub trait FileAdapter: GetMetadata {
    fn adapt(&self, a: AdaptInfo) -> Fallible<()>;
}
pub struct AdaptInfo<'a> {
    /// file path. May not be an actual file on the file system (e.g. in an archive). Used for matching file extensions.
    pub filepath_hint: &'a Path,
    /// true if filepath_hint is an actual file on the file system
    pub is_real_file: bool,
    /// depth at which this file is in archives. 0 for real filesystem
    pub archive_recursion_depth: i32,
    /// stream to read the file from. can be from a file or from some decoder
    pub inp: &'a mut dyn Read,
    /// stream to write to. will be written to from a different thread
    pub oup: &'a mut (dyn Write + Send),
    /// prefix every output line with this string to better indicate the file's location if it is in some archive
    pub line_prefix: &'a str,
    // pub adapt_subobject: &'a dyn Fn(AdaptInfo) -> Fallible<()>,
    pub config: PreprocConfig<'a>,
}

pub fn extension_to_regex(extension: &str) -> Regex {
    Regex::new(&format!(".*\\.{}", &regex::escape(extension))).expect("we know this regex compiles")
}

pub fn get_adapters() -> Vec<Rc<dyn FileAdapter>> {
    let adapters: Vec<Rc<dyn FileAdapter>> = vec![
        Rc::new(ffmpeg::FFmpegAdapter),
        Rc::new(pandoc::PandocAdapter),
        Rc::new(poppler::PopplerAdapter),
        Rc::new(zip::ZipAdapter),
        Rc::new(tar::TarAdapter),
        Rc::new(sqlite::SqliteAdapter),
    ];
    adapters
}

pub fn get_adapters_filtered(adapter_names: &Vec<String>) -> Fallible<Vec<Rc<dyn FileAdapter>>> {
    let all_adapters = get_adapters();
    let adapters = if !adapter_names.is_empty() {
        let adapters_map: HashMap<_, _> = all_adapters
            .iter()
            .map(|e| (e.metadata().name.clone(), e.clone()))
            .collect();
        let mut adapters = vec![];
        let mut subtractive = false;
        for (i, name) in adapter_names.iter().enumerate() {
            let mut name = &name[..];
            if i == 0 && name.starts_with("-") {
                subtractive = true;
                name = &name[1..];
                adapters = all_adapters.clone();
            }
            if subtractive {
                let inx = adapters
                    .iter()
                    .position(|a| &a.metadata().name == name)
                    .ok_or_else(|| format_err!("Could not remove {}: Not in list", name))?;
                adapters.remove(inx);
            } else {
                adapters.push(
                    adapters_map
                        .get(name)
                        .ok_or_else(|| format_err!("Unknown adapter: \"{}\"", name))?
                        .clone(),
                );
            }
        }
        adapters
    } else {
        all_adapters
    };
    debug!(
        "Chosen adapters: {}",
        adapters
            .iter()
            .map(|a| a.metadata().name.clone())
            .collect::<Vec<String>>()
            .join(",")
    );
    Ok(adapters)
}
pub fn adapter_matcher(
    adapter_names: &Vec<String>,
) -> Fallible<impl Fn(FileMeta) -> Option<Rc<dyn FileAdapter>>> {
    let adapters = get_adapters_filtered(adapter_names)?;
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
    Ok(move |meta: FileMeta| {
        // todo: handle multiple conflicting matches
        let matches = fname_regex_set.matches(&meta.lossy_filename);
        match matches.iter().next() {
            Some(m) => Some(fname_regexes[m].1.clone()),
            None => None,
        }
        /*for m in mime_regex_set.matches(&meta.mimetype) {
            return Some(mime_regexes[m].1.clone());
        }*/
    })
}
