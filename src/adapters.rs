pub mod custom;
pub mod decompress;
pub mod ffmpeg;
pub mod mbox;
pub mod postproc;
use std::sync::Arc;
pub mod sqlite;
pub mod tar;
pub mod writing;
pub mod zip;
use crate::{adapted_iter::AdaptedFilesIterBox, config::RgaConfig, matching::*};
use anyhow::{format_err, Context, Result};
use async_trait::async_trait;
use custom::CustomAdapterConfig;
use custom::BUILTIN_SPAWNING_ADAPTERS;
use log::*;
use tokio::io::AsyncRead;

use core::fmt::Debug;
use std::borrow::Cow;
use std::collections::HashMap;
use std::iter::Iterator;
use std::path::PathBuf;
use std::pin::Pin;

use self::postproc::PostprocPageBreaks;

pub type ReadBox = Pin<Box<dyn AsyncRead + Send>>;
pub struct AdapterMeta {
    /// unique short name of this adapter (a-z0-9 only)
    pub name: String,
    /// version identifier. used to key cache entries, change if your output format changes
    pub version: i32,
    pub description: String,
    /// indicates whether this adapter can descend (=call rga_preproc again). if true, the cache key needs to include the list of active adapters
    pub recurses: bool,
    /// list of matchers (interpreted as a OR b OR ...)
    pub fast_matchers: Vec<FastFileMatcher>,
    /// list of matchers when we have mime type detection active (interpreted as ORed)
    /// warning: this *overrides* the fast matchers
    pub slow_matchers: Option<Vec<FileMatcher>>,
    /// if true, slow_matchers is merged with fast matchers if accurate is enabled
    /// for example, in sqlite you want this disabled since the db extension can mean other things and the mime type matching is very accurate for sqlite.
    /// but for tar you want it enabled, since the tar extension is very accurate but the tar mime matcher can have false negatives
    pub keep_fast_matchers_if_accurate: bool,
    // if true, adapter is only used when user lists it in `--rga-adapters`
    pub disabled_by_default: bool,
}
impl AdapterMeta {
    // todo: this is pretty ugly
    pub fn get_matchers<'a>(
        &'a self,
        slow: bool,
    ) -> Box<dyn Iterator<Item = Cow<'a, FileMatcher>> + 'a> {
        match (
            slow,
            self.keep_fast_matchers_if_accurate,
            &self.slow_matchers,
        ) {
            (true, false, Some(ref sm)) => Box::new(sm.iter().map(Cow::Borrowed)),
            (true, true, Some(ref sm)) => Box::new(
                sm.iter().map(Cow::Borrowed).chain(
                    self.fast_matchers
                        .iter()
                        .map(|e| Cow::Owned(FileMatcher::Fast(e.clone()))),
                ),
            ),
            // don't have slow matchers or slow matching disabled
            (true, _, None) | (false, _, _) => Box::new(
                self.fast_matchers
                    .iter()
                    .map(|e| Cow::Owned(FileMatcher::Fast(e.clone()))),
            ),
        }
    }
}

pub trait GetMetadata {
    fn metadata(&self) -> &AdapterMeta;
}

#[async_trait]
pub trait FileAdapter: GetMetadata + Send + Sync {
    /// adapt a file.
    ///
    /// detection_reason is the Matcher that was used to identify this file. Unless --rga-accurate was given, it is always a FastMatcher
    async fn adapt(
        &self,
        a: AdaptInfo,
        detection_reason: &FileMatcher,
    ) -> Result<AdaptedFilesIterBox>;
}

pub struct AdaptInfo {
    /// file path. May not be an actual file on the file system (e.g. in an archive). Used for matching file extensions.
    pub filepath_hint: PathBuf,
    /// true if filepath_hint is an actual file on the file system
    pub is_real_file: bool,
    /// depth at which this file is in archives. 0 for real filesystem
    pub archive_recursion_depth: i32,
    /// stream to read the file from. can be from a file or from some decoder
    pub inp: ReadBox,
    /// prefix every output line with this string to better indicate the file's location if it is in some archive
    pub line_prefix: String,
    pub postprocess: bool,
    pub config: RgaConfig,
}

/// (enabledAdapters, disabledAdapters)
type AdaptersTuple = (Vec<Arc<dyn FileAdapter>>, Vec<Arc<dyn FileAdapter>>);

pub fn get_all_adapters(custom_adapters: Option<Vec<CustomAdapterConfig>>) -> AdaptersTuple {
    // order in descending priority
    let mut adapters: Vec<Arc<dyn FileAdapter>> = vec![];
    if let Some(custom_adapters) = custom_adapters {
        for adapter_config in custom_adapters {
            adapters.push(Arc::new(adapter_config.to_adapter()));
        }
    }

    let internal_adapters: Vec<Arc<dyn FileAdapter>> = vec![
        Arc::new(PostprocPageBreaks::default()),
        Arc::new(ffmpeg::FFmpegAdapter::new()),
        Arc::new(zip::ZipAdapter::new()),
        Arc::new(decompress::DecompressAdapter::new()),
        Arc::new(mbox::MboxAdapter::new()),
        Arc::new(tar::TarAdapter::new()),
        Arc::new(sqlite::SqliteAdapter::new()),
    ];
    adapters.extend(
        BUILTIN_SPAWNING_ADAPTERS
            .iter()
            .map(|e| -> Arc<dyn FileAdapter> { Arc::new(e.to_adapter()) }),
    );
    adapters.extend(internal_adapters);

    adapters
        .into_iter()
        .partition(|e| !e.metadata().disabled_by_default)
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
    custom_adapters: Option<Vec<CustomAdapterConfig>>,
    adapter_names: &[T],
) -> Result<Vec<Arc<dyn FileAdapter>>> {
    let (def_enabled_adapters, def_disabled_adapters) = get_all_adapters(custom_adapters);
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
                    .ok_or_else(|| format_err!("Could not remove adapter {}: Not in list", name))?;
                adapters.remove(inx);
            } else {
                let adapter = adapters_map
                    .get(name)
                    .ok_or_else(|| {
                        format_err!(
                            "Unknown adapter: \"{}\". Known adapters: {}",
                            name,
                            adapters_map
                                .keys()
                                .map(|e| e.as_ref())
                                .collect::<Vec<&str>>()
                                .join(", ")
                        )
                    })?
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
        "Chosen available adapters: {}",
        adapters
            .iter()
            .map(|a| a.metadata().name.clone())
            .collect::<Vec<String>>()
            .join(",")
    );
    Ok(adapters)
}
