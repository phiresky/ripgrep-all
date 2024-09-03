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
use custom::Builtin;
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
    pub slow_matchers: Vec<FileMatcher>,
    /// if true, slow_matchers is merged with fast matchers if accurate is enabled
    /// for example, in sqlite you want this disabled since the db extension can mean other things and the mime type matching is very accurate for sqlite.
    /// but for tar you want it enabled, since the tar extension is very accurate but the tar mime matcher can have false negatives
    pub keep_fast_matchers_if_accurate: bool,
    // if true, adapter is only used when user lists it in `--rga-adapters`
    pub disabled_by_default: bool,
}
impl AdapterMeta {
    // todo: this is pretty ugly
    pub fn get_matchers(&self, slow: bool) -> Box<dyn Iterator<Item = Cow<FileMatcher>> + '_> {
        match (
            slow,
            self.keep_fast_matchers_if_accurate,
            &self.slow_matchers,
            &self.fast_matchers,
        ) {
            (true, false, sm, _) => Box::new(sm.iter().map(Cow::Borrowed)),
            (true, true, sm, fm) => Box::new(
                sm.iter().map(Cow::Borrowed).chain(
                    fm.iter()
                        .map(|e| Cow::Owned(FileMatcher::Fast(e.clone())))
                        .collect::<Vec<_>>(),
                ),
            ),
            // slow matching disabled
            (false, _, _, fm) => {
                Box::new(fm.iter().map(|e| Cow::Owned(FileMatcher::Fast(e.clone()))))
            }
        }
    }
}

pub trait Adapter {
    fn name(&self) -> String;
    fn version(&self) -> i32;
    fn description(&self) -> String;
    fn recurses(&self) -> bool;
    fn disabled_by_default(&self) -> bool;
    fn keep_fast_matchers_if_accurate(&self) -> bool;
    fn extensions(&self) -> Vec<String>;
    fn mimetypes(&self) -> Vec<String>;

    fn metadata(&self) -> AdapterMeta {
        return AdapterMeta {
            name: self.name(),
            version: self.version(),
            description: self.description(),
            recurses: true,
            fast_matchers: self
                .extensions()
                .iter()
                .map(|s| FastFileMatcher::FileExtension(s.to_string()))
                .collect(),
            slow_matchers: self
                .mimetypes()
                .iter()
                .map(|mimetype| FileMatcher::MimeType(mimetype.to_string()))
                .collect(),
            disabled_by_default: self.disabled_by_default(),
            keep_fast_matchers_if_accurate: self.keep_fast_matchers_if_accurate(),
        };
    }
}

#[async_trait]
pub trait FileAdapter: Adapter + Send + Sync {
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

pub fn get_all_adapters(
    custom_extensions: Option<HashMap<String, Builtin>>,
    custom_mimetypes: Option<HashMap<String, Builtin>>,
    custom_adapters: Option<Vec<CustomAdapterConfig>>,
) -> AdaptersTuple {
    let extensions: &mut HashMap<Builtin, Vec<String>> = &mut HashMap::new();
    if let Some(ce) = custom_extensions.as_ref() {
        for (ext, builtin) in ce {
            extensions
                .entry(*builtin)
                .or_default()
                .push(ext.to_string());
        }
    }
    for (builtin, exts) in [
        (Builtin::BZ2, decompress::EXTENSIONS_BZ2),
        (Builtin::GZ, decompress::EXTENSIONS_GZ),
        (Builtin::XZ, decompress::EXTENSIONS_XZ),
        (Builtin::ZST, decompress::EXTENSIONS_ZST),
        (Builtin::FFMPEG, ffmpeg::EXTENSIONS),
        (Builtin::MBOX, mbox::EXTENSIONS),
        (Builtin::SQLITE, sqlite::EXTENSIONS),
        (Builtin::TAR, tar::EXTENSIONS),
        (Builtin::ZIP, zip::EXTENSIONS),
    ] {
        for ext in exts {
            if !custom_extensions
                .as_ref()
                .is_some_and(|ce| ce.contains_key(ext.to_owned()))
            {
                extensions.entry(builtin).or_default().push(ext.to_string());
            }
        }
    }

    let mimetypes: &mut HashMap<Builtin, Vec<String>> = &mut HashMap::new();
    if let Some(cm) = custom_mimetypes.as_ref() {
        for (mime, builtin) in cm {
            mimetypes
                .entry(*builtin)
                .or_default()
                .push(mime.to_string());
        }
    }
    for (builtin, mimes) in [
        (Builtin::BZ2, decompress::MIMETYPES_BZ2),
        (Builtin::GZ, decompress::MIMETYPES_GZ),
        (Builtin::XZ, decompress::MIMETYPES_XZ),
        (Builtin::ZST, decompress::MIMETYPES_ZST),
        (Builtin::FFMPEG, ffmpeg::MIMETYPES),
        (Builtin::MBOX, mbox::MIMETYPES),
        (Builtin::SQLITE, sqlite::MIMETYPES),
        (Builtin::TAR, tar::MIMETYPES),
        (Builtin::ZIP, zip::MIMETYPES),
    ] {
        let val = mimetypes.entry(builtin).or_default();
        for mime in mimes {
            val.push(mime.to_string());
        }
    }

    // order in descending priority
    let mut adapters: Vec<Arc<dyn FileAdapter>> = vec![];
    if let Some(custom_adapters) = custom_adapters {
        for adapter_config in custom_adapters {
            adapters.push(Arc::new(adapter_config.to_adapter()));
        }
    }

    let internal_adapters: Vec<Arc<dyn FileAdapter>> = vec![
        Arc::new(PostprocPageBreaks::default()),
        Arc::new(ffmpeg::FFmpegAdapter {
            extensions: extensions[&Builtin::FFMPEG].clone(),
            mimetypes: mimetypes[&Builtin::FFMPEG].clone(),
        }),
        Arc::new(zip::ZipAdapter {
            extensions: extensions[&Builtin::ZIP].clone(),
            mimetypes: mimetypes[&Builtin::ZIP].clone(),
        }),
        Arc::new(decompress::DecompressAdapter {
            extensions_gz: extensions[&Builtin::GZ].clone(),
            extensions_bz2: extensions[&Builtin::BZ2].clone(),
            extensions_xz: extensions[&Builtin::XZ].clone(),
            extensions_zst: extensions[&Builtin::ZST].clone(),
            mimetypes_gz: mimetypes[&Builtin::GZ].clone(),
            mimetypes_bz2: mimetypes[&Builtin::BZ2].clone(),
            mimetypes_xz: mimetypes[&Builtin::XZ].clone(),
            mimetypes_zst: mimetypes[&Builtin::ZST].clone(),
        }),
        Arc::new(mbox::MboxAdapter {
            extensions: extensions[&Builtin::MBOX].clone(),
            mimetypes: mimetypes[&Builtin::MBOX].clone(),
        }),
        Arc::new(sqlite::SqliteAdapter {
            extensions: extensions[&Builtin::SQLITE].clone(),
            mimetypes: mimetypes[&Builtin::SQLITE].clone(),
        }),
        Arc::new(tar::TarAdapter {
            extensions: extensions[&Builtin::TAR].clone(),
            mimetypes: mimetypes[&Builtin::TAR].clone(),
        }),
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
    custom_extensions: Option<HashMap<String, Builtin>>,
    custom_identifiers: Option<HashMap<String, Builtin>>,
    custom_adapters: Option<Vec<CustomAdapterConfig>>,
    adapter_names: &[T],
) -> Result<Vec<Arc<dyn FileAdapter>>> {
    let (def_enabled_adapters, def_disabled_adapters) =
        get_all_adapters(custom_extensions, custom_identifiers, custom_adapters);
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
