/**
 * Module for matching adapters to files based on file name or mime type
 */
use crate::adapters::*;

use anyhow::*;

use regex::Regex;

use std::iter::Iterator;

use std::sync::Arc;

// match only based on file path
#[derive(Clone, Debug)]
pub enum FastFileMatcher {
    // MimeType(Regex),
    /**
     * without the leading dot, e.g. "jpg" or "tar.gz". Matched as /.*\.ext$/
     *
     */
    FileExtension(String),
    // todo: maybe add others, e.g. regex on whole filename or even paths
    // todo: maybe allow matching a directory (e.g. /var/lib/postgres)
}

#[derive(Clone, Debug)]
pub enum FileMatcher {
    /// any type of fast matcher
    Fast(FastFileMatcher),
    ///
    /// match by exact mime type extracted using tree_magic
    /// TODO: allow match ignoring suffix etc?
    MimeType(String),
}

impl From<FastFileMatcher> for FileMatcher {
    fn from(t: FastFileMatcher) -> Self {
        Self::Fast(t)
    }
}

pub struct FileMeta {
    // filename is not actually a utf8 string, but since we can't do regex on OsStr and can't get a &[u8] from OsStr either,
    // and since we probably only want to do only matching on ascii stuff anyways, this is the filename as a string with non-valid bytes removed
    pub lossy_filename: String,
    // only given when slow matching is enabled
    pub mimetype: Option<&'static str>,
}

pub fn extension_to_regex(extension: &str) -> Regex {
    Regex::new(&format!("(?i)\\.{}$", &regex::escape(extension)))
        .expect("we know this regex compiles")
}

#[allow(clippy::type_complexity)]
pub fn adapter_matcher(
    adapters: &[Arc<dyn FileAdapter>],
    slow: bool,
) -> Result<Box<dyn Fn(FileMeta) -> Option<(Arc<dyn FileAdapter>, FileMatcher)> + Send + Sync>> {
    let adapter_names: Vec<String> = adapters.iter().map(|e| e.metadata().name.clone()).collect();
    let mut ext_map: std::collections::HashMap<String, Vec<(Arc<dyn FileAdapter>, FileMatcher)>> =
        std::collections::HashMap::new();
    let mut mime_map: std::collections::HashMap<String, Vec<(Arc<dyn FileAdapter>, FileMatcher)>> =
        std::collections::HashMap::new();
    for adapter in adapters.iter() {
        let metadata = adapter.metadata();
        for matcher in metadata.get_matchers(slow) {
            match matcher.as_ref() {
                FileMatcher::MimeType(m) => {
                    let k = m.to_string();
                    mime_map
                        .entry(k)
                        .or_default()
                        .push((adapter.clone(), FileMatcher::MimeType(m.clone())));
                }
                FileMatcher::Fast(FastFileMatcher::FileExtension(ext)) => {
                    let k = ext.to_ascii_lowercase();
                    ext_map.entry(k).or_default().push((
                        adapter.clone(),
                        FileMatcher::Fast(FastFileMatcher::FileExtension(ext.clone())),
                    ));
                }
            }
        }
    }
    let func = move |meta: FileMeta| {
        let ext = std::path::Path::new(&meta.lossy_filename)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase());
        let mut candidates: Vec<(Arc<dyn FileAdapter>, FileMatcher)> = vec![];
        if let Some(ext) = ext
            && let Some(v) = ext_map.get(&ext)
        {
            candidates.extend(v.iter().cloned());
        }
        if slow
            && let Some(mt) = meta.mimetype
            && let Some(v) = mime_map.get(mt)
        {
            candidates.extend(v.iter().cloned());
        }
        if candidates.is_empty() {
            return None;
        }
        if candidates.len() > 1 {
            candidates.sort_by_key(|e| {
                adapter_names
                    .iter()
                    .position(|r| r == &e.0.metadata().name)
                    .unwrap_or(usize::MAX)
            });
            eprintln!(
                "Warning: found multiple adapters for {}:",
                meta.lossy_filename
            );
            for mmatch in candidates.iter() {
                eprintln!(" - {}", mmatch.0.metadata().name);
            }
        }
        Some(candidates.remove(0))
    };
    Ok(Box::new(func))
}
