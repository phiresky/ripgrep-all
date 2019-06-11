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

use std::borrow::Cow;
use std::collections::HashMap;
use std::io::prelude::*;
use std::iter::Iterator;
use std::path::Path;
use std::rc::Rc;

#[derive(Clone)]
pub enum FastMatcher {
    // MimeType(Regex),
    /**
     * without the leading dot, e.g. "jpg" or "tar.gz". Matched as /.*\.ext$/
     *
     */
    FileExtension(String),
    // todo: maybe add others, e.g. regex on whole filename or even paths
    // todo: maybe allow matching a directory (e.g. /var/lib/postgres)
}

#[derive(Clone)]
pub enum SlowMatcher {
    /// any type of fast matcher
    Fast(FastMatcher),
    ///
    /// match by exact mime type extracted using tree_magic
    /// TODO: allow match ignoring suffix etc?
    MimeType(String),
}

pub struct AdapterMeta {
    /// unique short name of this adapter (a-z0-9 only)
    pub name: String,
    /// version identifier. used to key cache entries, change if your output format changes
    pub version: i32,
    pub description: String,
    /// list of matchers (interpreted as ORed)
    pub fast_matchers: Vec<FastMatcher>,
    /// list of matchers when we have mime type detection active (interpreted as ORed)
    /// warning: this *overrides* the fast matchers
    pub slow_matchers: Option<Vec<SlowMatcher>>,
}
impl AdapterMeta {
    // todo: this is pretty ugly
    fn get_matchers<'a>(&'a self, slow: bool) -> Box<dyn Iterator<Item = Cow<SlowMatcher>> + 'a> {
        match (slow, &self.slow_matchers) {
            (true, Some(ref sm)) => Box::new(sm.iter().map(|e| Cow::Borrowed(e))),
            (_, _) => Box::new(
                self.fast_matchers
                    .iter()
                    .map(|e| Cow::Owned(SlowMatcher::Fast(e.clone()))),
            ),
        }
    }
}

pub struct FileMeta {
    // filename is not actually a utf8 string, but since we can't do regex on OsStr and can't get a &[u8] from OsStr either,
    // and since we probably only want to do only matching on ascii stuff anyways, this is the filename as a string with non-valid bytes removed
    pub lossy_filename: String,
    // only given when slow matching is enabled
    pub mimetype: Option<String>,
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
    // order in descending priority
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

pub fn get_adapters_filtered<T: AsRef<str>>(
    adapter_names: &[T],
) -> Fallible<Vec<Rc<dyn FileAdapter>>> {
    let all_adapters = get_adapters();
    let adapters = if !adapter_names.is_empty() {
        let adapters_map: HashMap<_, _> = all_adapters
            .iter()
            .map(|e| (e.metadata().name.clone(), e.clone()))
            .collect();
        let mut adapters = vec![];
        let mut subtractive = false;
        for (i, name) in adapter_names.iter().enumerate() {
            let mut name = name.as_ref();
            if i == 0 && (name.starts_with('-')) {
                subtractive = true;
                name = &name[1..];
                adapters = all_adapters.clone();
            }
            if subtractive {
                let inx = adapters
                    .iter()
                    .position(|a| a.metadata().name == name)
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

pub fn adapter_matcher<T: AsRef<str>>(
    adapter_names: &[T],
    slow: bool,
) -> Fallible<impl Fn(FileMeta) -> Option<Rc<dyn FileAdapter>>> {
    let adapters = get_adapters_filtered(adapter_names)?;
    // need order later
    let adapter_names: Vec<String> = adapters.iter().map(|e| e.metadata().name.clone()).collect();
    let mut fname_regexes = vec![];
    let mut mime_regexes = vec![];
    for adapter in adapters.into_iter() {
        let metadata = adapter.metadata();
        use SlowMatcher::*;
        for matcher in metadata.get_matchers(slow) {
            match matcher.as_ref() {
                MimeType(re) => mime_regexes.push((re.clone(), adapter.clone())),
                Fast(FastMatcher::FileExtension(re)) => {
                    fname_regexes.push((extension_to_regex(re), adapter.clone()))
                }
            };
        }
    }
    let fname_regex_set = RegexSet::new(fname_regexes.iter().map(|p| p.0.as_str()))?;
    let mime_regex_set = RegexSet::new(mime_regexes.iter().map(|p| p.0.as_str()))?;
    Ok(move |meta: FileMeta| {
        let fname_matches: Vec<_> = fname_regex_set
            .matches(&meta.lossy_filename)
            .into_iter()
            .collect();
        let mime_matches: Vec<_> = if slow {
            mime_regex_set
                .matches(&meta.mimetype.expect("No mimetype?"))
                .into_iter()
                .collect()
        } else {
            vec![]
        };
        if fname_matches.len() + mime_matches.len() > 1 {
            // get first according to original priority list...
            let fa = fname_matches.iter().map(|e| fname_regexes[*e].1.clone());
            let fb = mime_matches.iter().map(|e| mime_regexes[*e].1.clone());
            let mut v = vec![];
            v.extend(fa);
            v.extend(fb);
            v.sort_by_key(|e| {
                (adapter_names
                    .iter()
                    .position(|r| r == &e.metadata().name)
                    .expect("impossib7"))
            });
            eprintln!(
                "Warning: found multiple adapters for {}:",
                meta.lossy_filename
            );
            for mmatch in v.iter() {
                eprintln!(" - {}", mmatch.metadata().name);
            }
            return Some(v[0].clone());
        }
        if mime_matches.is_empty() {
            if fname_matches.is_empty() {
                None
            } else {
                Some(fname_regexes[fname_matches[0]].1.clone())
            }
        } else {
            Some(mime_regexes[mime_matches[0]].1.clone())
        }
    })
}
