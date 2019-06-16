pub mod decompress;
pub mod ffmpeg;
pub mod pandoc;
pub mod pdfpages;
pub mod poppler;
pub mod spawning;
pub mod sqlite;
pub mod tar;
pub mod tesseract;
pub mod zip;
use crate::matching::*;
use crate::preproc::PreprocConfig;
use failure::*;
use log::*;
use regex::Regex;
use std::borrow::Cow;
use std::collections::HashMap;
use std::io::prelude::*;
use std::iter::Iterator;
use std::path::Path;
use std::rc::Rc;

pub struct AdapterMeta {
    /// unique short name of this adapter (a-z0-9 only)
    pub name: String,
    /// version identifier. used to key cache entries, change if your output format changes
    pub version: i32,
    pub description: String,
    /// indicates whether this adapter can descend (=call rga_preproc again). if true, the cache key needs to include the list of active adapters
    pub recurses: bool,
    /// list of matchers (interpreted as a OR b OR ...)
    pub fast_matchers: Vec<FastMatcher>,
    /// list of matchers when we have mime type detection active (interpreted as ORed)
    /// warning: this *overrides* the fast matchers
    pub slow_matchers: Option<Vec<SlowMatcher>>,
}
impl AdapterMeta {
    // todo: this is pretty ugly
    pub fn get_matchers<'a>(
        &'a self,
        slow: bool,
    ) -> Box<dyn Iterator<Item = Cow<SlowMatcher>> + 'a> {
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

pub trait GetMetadata {
    fn metadata(&self) -> &AdapterMeta;
}
pub trait FileAdapter: GetMetadata {
    /// adapt a file.
    ///
    /// detection_reason is the Matcher that was used to identify this file. Unless --rga-accurate was given, it is always a FastMatcher
    fn adapt(&self, a: AdaptInfo, detection_reason: &SlowMatcher) -> Fallible<()>;
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
    pub config: PreprocConfig<'a>,
}

/// (enabledAdapters, disabledAdapters)
type AdaptersTuple = (Vec<Rc<dyn FileAdapter>>, Vec<Rc<dyn FileAdapter>>);

pub fn get_all_adapters() -> AdaptersTuple {
    // order in descending priority
    let enabled_adapters: Vec<Rc<dyn FileAdapter>> = vec![
        Rc::new(ffmpeg::FFmpegAdapter::new()),
        Rc::new(pandoc::PandocAdapter::new()),
        Rc::new(poppler::PopplerAdapter::new()),
        Rc::new(zip::ZipAdapter::new()),
        Rc::new(decompress::DecompressAdapter::new()),
        Rc::new(tar::TarAdapter::new()),
        Rc::new(sqlite::SqliteAdapter::new()),
    ];
    let disabled_adapters: Vec<Rc<dyn FileAdapter>> = vec![
        Rc::new(pdfpages::PdfPagesAdapter::new()),
        Rc::new(tesseract::TesseractAdapter::new()),
    ];
    (enabled_adapters, disabled_adapters)
}

/**
 * filter adapters by given names:
 *
 *  - "" means use default enabled adapter list
 *  - "a,b" means use adapters a,b
 *  - "-a,b" means use default list except for a and b
 *  - "+a,b" means use default list but also a and b (a,b will be prepended to the list so given higher priority)
 */
pub fn get_adapters_filtered<T: AsRef<str>>(
    adapter_names: &[T],
) -> Fallible<Vec<Rc<dyn FileAdapter>>> {
    let (def_enabled_adapters, def_disabled_adapters) = get_all_adapters();
    let adapters = if !adapter_names.is_empty() {
        let adapters_map: HashMap<_, _> = def_enabled_adapters
            .iter()
            .chain(def_disabled_adapters.iter())
            .map(|e| (e.metadata().name.clone(), e.clone()))
            .collect();
        let mut adapters = vec![];
        let mut subtractive = false;
        let mut additive = false;
        for (i, name) in adapter_names.iter().enumerate() {
            let mut name = name.as_ref();
            if i == 0 && (name.starts_with('-')) {
                subtractive = true;
                name = &name[1..];
                adapters = def_enabled_adapters.clone();
            } else if i == 0 && (name.starts_with('+')) {
                name = &name[1..];
                adapters = def_enabled_adapters.clone();
                additive = true;
            }
            if subtractive {
                let inx = adapters
                    .iter()
                    .position(|a| a.metadata().name == name)
                    .ok_or_else(|| format_err!("Could not remove {}: Not in list", name))?;
                adapters.remove(inx);
            } else {
                let adapter = adapters_map
                    .get(name)
                    .ok_or_else(|| format_err!("Unknown adapter: \"{}\"", name))?
                    .clone();
                if additive {
                    adapters.insert(0, adapter);
                } else {
                    adapters.push(adapter);
                }
            }
        }
        adapters
    } else {
        def_enabled_adapters
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
