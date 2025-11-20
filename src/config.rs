use crate::{adapters::custom::CustomAdapterConfig, project_dirs};
use anyhow::{Context, Result};
use derive_more::FromStr;
use log::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::io::Read;
use std::{fs::File, io::Write, iter::IntoIterator, path::PathBuf, str::FromStr};
use clap::Parser;
use serde::Deserialize as DeDeserialize;

fn is_default<T: Default + PartialEq>(t: &T) -> bool {
    t == &T::default()
}
#[derive(JsonSchema, Debug, Serialize, Deserialize, Copy, Clone, PartialEq, FromStr)]
pub struct CacheCompressionLevel(pub i32);

impl std::fmt::Display for CacheCompressionLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl Default for CacheCompressionLevel {
    fn default() -> Self {
        Self(8)
    }
}
#[derive(JsonSchema, Debug, Serialize, Deserialize, Copy, Clone, PartialEq, FromStr)]
pub struct MaxArchiveRecursion(pub i32);

impl std::fmt::Display for MaxArchiveRecursion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl Default for MaxArchiveRecursion {
    fn default() -> Self {
        Self(5)
    }
}

#[derive(JsonSchema, Debug, Serialize, Deserialize, Copy, Clone, PartialEq, FromStr)]
pub struct ZipMaxConcurrency(pub usize);
impl std::fmt::Display for ZipMaxConcurrency { fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.0) } }
impl Default for ZipMaxConcurrency {


    fn default() -> Self {
        Self(8)
    }
}

#[derive(JsonSchema, Debug, Serialize, Deserialize, Copy, Clone, PartialEq, FromStr)]
pub struct ZipPipeBytes(pub usize);
impl std::fmt::Display for ZipPipeBytes { fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.0) } }
impl Default for ZipPipeBytes {
    fn default() -> Self {
        Self(524288)
    }
}

#[derive(JsonSchema, Debug, Serialize, Deserialize, Copy, Clone, PartialEq, FromStr)]
pub struct TarMaxConcurrency(pub usize);
impl std::fmt::Display for TarMaxConcurrency { fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.0) } }
impl Default for TarMaxConcurrency { fn default() -> Self { Self(1) } }

#[derive(JsonSchema, Debug, Serialize, Deserialize, Copy, Clone, PartialEq, FromStr)]
pub struct TarPipeBytes(pub usize);
impl std::fmt::Display for TarPipeBytes { fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.0) } }
impl Default for TarPipeBytes { fn default() -> Self { Self(524288) } }

#[derive(JsonSchema, Debug, Serialize, Deserialize, Copy, Clone, PartialEq, FromStr)]
pub struct WritingPipeBytes(pub usize);
impl std::fmt::Display for WritingPipeBytes { fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.0) } }
impl Default for WritingPipeBytes { fn default() -> Self { Self(512 * 1024) } }

#[derive(JsonSchema, Debug, Serialize, Deserialize, Copy, Clone, PartialEq, FromStr)]
pub struct PostprocPipeBytes(pub usize);
impl std::fmt::Display for PostprocPipeBytes { fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.0) } }
impl Default for PostprocPipeBytes {
    fn default() -> Self {
        Self(524288)
    }
}
#[derive(JsonSchema, Debug, Serialize, Deserialize, Copy, Clone, PartialEq, FromStr)]
pub struct DecompressGzipBufBytes(pub usize);

impl std::fmt::Display for DecompressGzipBufBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl Default for DecompressGzipBufBytes {
    fn default() -> Self {
        Self(524288)
    }
}



#[derive(JsonSchema, Debug, Serialize, Deserialize, Copy, Clone, PartialEq, FromStr)]
pub struct DecompressBzip2BufBytes(pub usize);

impl std::fmt::Display for DecompressBzip2BufBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl Default for DecompressBzip2BufBytes {
    fn default() -> Self {
        Self(262144)
    }
}

#[derive(JsonSchema, Debug, Serialize, Deserialize, Copy, Clone, PartialEq, FromStr)]
pub struct DecompressXzBufBytes(pub usize);

impl std::fmt::Display for DecompressXzBufBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl Default for DecompressXzBufBytes {
    fn default() -> Self {
        Self(262144)
    }
}



#[derive(JsonSchema, Debug, Serialize, Deserialize, Copy, Clone, PartialEq, FromStr)]
pub struct DecompressZstdBufBytes(pub usize);

impl std::fmt::Display for DecompressZstdBufBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl Default for DecompressZstdBufBytes {
    fn default() -> Self {
        Self(524288)
    }
}


#[derive(JsonSchema, Debug, Serialize, Deserialize, Copy, Clone, PartialEq, FromStr, Default)]
pub struct MemCapBytes(pub usize);

impl std::fmt::Display for MemCapBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
 


#[derive(JsonSchema, Debug, Serialize, Deserialize, Copy, Clone, PartialEq, FromStr, Default)]
pub struct CacheSmallUncompressedBytes(pub usize);

impl std::fmt::Display for CacheSmallUncompressedBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
// default derives from #[derive(Default)] above

#[derive(JsonSchema, Debug, Serialize, Deserialize, Clone, PartialEq, FromStr)]
pub struct CachePath(pub String);

impl std::fmt::Display for CachePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl Default for CachePath {
    fn default() -> Self {
        let pd = project_dirs().expect("could not get cache path");
        let app_cache = pd.cache_dir();
        Self(app_cache.to_str().expect("cache path not utf8").to_owned())
    }
}

#[derive(JsonSchema, Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq)]
pub struct CacheMaxBlobLen(pub usize);

impl std::fmt::Display for CacheMaxBlobLen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl Default for CacheMaxBlobLen {
    fn default() -> Self {
        Self(2000000)
    }
}

impl FromStr for CacheMaxBlobLen {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let suffix = s.chars().last();
        if let Some(suffix) = suffix {
            Ok(Self(match suffix {
                'k' | 'M' | 'G' => usize::from_str(s.trim_end_matches(suffix))
                    .with_context(|| "Could not parse int".to_string())
                    .map(|e| {
                        e * match suffix {
                            'k' => 1000,
                            'M' => 1_000_000,
                            'G' => 1_000_000_000,
                            _ => panic!("impossible"),
                        }
                    }),
                _ => usize::from_str(s).with_context(|| "Could not parse int".to_string()),
            }?))
        } else {
            Err(anyhow::format_err!("empty byte input"))
        }
    }
}

/// # rga configuration
///
/// This is kind of a "polyglot" struct serving multiple purposes:
///
/// 1. Declare the command line arguments using structopt+clap
/// 1. Provide information for manpage / readme generation.
/// 1. Describe the config file format (output as JSON schema via schemars).
#[derive(Parser, Debug, Deserialize, Serialize, JsonSchema, Default, Clone)]
#[command(
    name = "ripgrep-all",
    rename_all = "kebab-case",
    about = env!("CARGO_PKG_DESCRIPTION"),
    author = env!("CARGO_PKG_HOMEPAGE"),
    long_about="rga: ripgrep, but also search in PDFs, E-Books, Office documents, zip, tar.gz, etc.",
    after_help = "-h shows a concise overview, --help shows more detail and advanced options.\n\nAll other options not shown here are passed directly to rg, especially [PATTERN] and [PATH ...]",
    override_usage = "rga [RGA OPTIONS] [RG OPTIONS] PATTERN [PATH ...]"
)]
pub struct RgaConfig {
    /// Use more accurate but slower matching by mime type.
    ///
    /// By default, rga will match files using file extensions.
    /// Some programs, such as sqlite3, don't care about the file extension at all, so users sometimes use any or no extension at all.
    /// With this flag, rga will try to detect the mime type of input files using the magic bytes (similar to the `file` utility), and use that to choose the adapter.
    /// Detection is only done on the first 8KiB of the file, since we can't always seek on the input (in archives).
    #[serde(default, skip_serializing_if = "is_default")]
    #[arg(long = "rga-accurate")]
    pub accurate: bool,

    /// Change which adapters to use and in which priority order (descending).
    ///
    /// - "foo,bar" means use only adapters foo and bar.
    /// - "-bar,baz" means use all default adapters except for bar and baz.
    /// - "+bar,baz" means use all default adapters and also bar and baz.
    #[serde(default, skip_serializing_if = "is_default")]
    #[arg(
        long = "rga-adapters",
        require_equals = true,
        value_delimiter = ','
    )]
    pub adapters: Vec<String>,

    #[serde(default, skip_serializing_if = "is_default")]
    #[command(flatten)]
    pub cache: CacheConfig,

    /// Maximum depth of nested archives to recurse into.
    ///
    /// When searching in archives, rga will recurse into archives inside archives.
    /// This option limits the depth.
    #[serde(default, skip_serializing_if = "is_default")]
    #[arg(
        default_value_t,
        long = "rga-max-archive-recursion",
        require_equals = true,
        hide_short_help = true
    )]
    pub max_archive_recursion: MaxArchiveRecursion,

    /// Don't prefix lines of files within archive with the path inside the archive.
    ///
    /// Inside archives, by default rga prefixes the content of each file with the file path within the archive.
    /// This is usually useful, but can cause problems because then the inner path is also searched for the pattern.
    #[serde(default, skip_serializing_if = "is_default")]
    #[arg(long = "rga-no-prefix-filenames")]
    pub no_prefix_filenames: bool,

    #[serde(default, skip_serializing_if = "is_default")]
    #[arg(
        default_value_t,
        long = "rga-zip-max-concurrency",
        require_equals = true,
        hide_short_help = true
    )]
    pub zip_max_concurrency: ZipMaxConcurrency,

    #[serde(default, skip_serializing_if = "is_default")]
    #[arg(
        default_value_t,
        long = "rga-zip-pipe-bytes",
        require_equals = true,
        hide_short_help = true
    )]
    pub zip_pipe_bytes: ZipPipeBytes,

    #[serde(default, skip_serializing_if = "is_default")]
    #[arg(
        default_value_t,
        long = "rga-writing-pipe-bytes",
        require_equals = true,
        hide_short_help = true
    )]
    pub writing_pipe_bytes: WritingPipeBytes,

    #[serde(default, skip_serializing_if = "is_default")]
    #[arg(
        default_value_t,
        long = "rga-postproc-pipe-bytes",
        require_equals = true,
        hide_short_help = true
    )]
    pub postproc_pipe_bytes: PostprocPipeBytes,

    #[serde(default, skip_serializing_if = "is_default")]
    #[arg(long = "rga-zip-owned-iter")]
    pub zip_owned_iter: bool,

    #[serde(default, skip_serializing_if = "is_default")]
    #[arg(
        default_value_t,
        long = "rga-tar-max-concurrency",
        require_equals = true,
        hide_short_help = true
    )]
    pub tar_max_concurrency: TarMaxConcurrency,

    #[serde(default, skip_serializing_if = "is_default")]
    #[arg(
        default_value_t,
        long = "rga-tar-pipe-bytes",
        require_equals = true,
        hide_short_help = true
    )]
    pub tar_pipe_bytes: TarPipeBytes,

    #[serde(default, skip_serializing_if = "is_default")]
    #[arg(
        default_value_t,
        long = "rga-decompress-gzip-buf-bytes",
        require_equals = true,
        hide_short_help = true
    )]
    pub decompress_gzip_buf_bytes: DecompressGzipBufBytes,

    #[serde(default, skip_serializing_if = "is_default")]
    #[arg(
        default_value_t,
        long = "rga-decompress-bzip2-buf-bytes",
        require_equals = true,
        hide_short_help = true
    )]
    pub decompress_bzip2_buf_bytes: DecompressBzip2BufBytes,

    #[serde(default, skip_serializing_if = "is_default")]
    #[arg(
        default_value_t,
        long = "rga-decompress-xz-buf-bytes",
        require_equals = true,
        hide_short_help = true
    )]
    pub decompress_xz_buf_bytes: DecompressXzBufBytes,

    #[serde(default, skip_serializing_if = "is_default")]
    #[arg(
        default_value_t,
        long = "rga-decompress-zstd-buf-bytes",
        require_equals = true,
        hide_short_help = true
    )]
    pub decompress_zstd_buf_bytes: DecompressZstdBufBytes,

    #[serde(default, skip_serializing_if = "is_default")]
    #[arg(long = "rga-decompress-autotune")]
    pub decompress_autotune: bool,

    #[serde(default, skip_serializing_if = "is_default")]
    #[arg(long = "rga-decompress-autotune-export", require_equals = true)]
    pub decompress_autotune_export: Option<String>,

    #[serde(default, skip_serializing_if = "is_default")]
    #[arg(long = "rga-decompress-autotune-import", require_equals = true)]
    pub decompress_autotune_import: Option<String>,

    #[serde(default, skip_serializing_if = "is_default")]
    #[arg(skip)] // config file only
    pub custom_adapters: Option<Vec<CustomAdapterConfig>>,

    #[serde(skip)]
    #[arg(long = "rga-config-file", require_equals = true)]
    pub config_file_path: Option<String>,

    #[serde(skip)]
    #[arg(long = "rga-load-sweep", require_equals = true)]
    pub load_sweep_path: Option<String>,

    /// Same as passing path directly, except if argument is empty.
    ///
    /// Kinda hacky, but if no file is found, `fzf` calls `rga` with empty string as path, which causes "No such file or directory from rg".
    /// So filter those cases and return specially.
    #[serde(skip)] // CLI only
    #[arg(long = "rga-fzf-path", require_equals = true, hide = true)]
    pub fzf_path: Option<String>,

    #[serde(skip)] // CLI only
    #[arg(long = "rga-list-adapters", help = "List all known adapters")]
    pub list_adapters: bool,

    #[serde(skip)]
    #[arg(long = "rga-cache-build")]
    pub cache_build: bool,

    #[serde(skip)] // CLI only
    #[arg(
        long = "rga-print-config-schema",
        help = "Print the JSON Schema of the configuration file"
    )]
    pub print_config_schema: bool,

    #[serde(skip)] // CLI only
    #[arg(long, help = "Show help for ripgrep itself")]
    pub rg_help: bool,

    #[serde(skip)] // CLI only
    #[arg(long, help = "Show version of ripgrep itself")]
    pub rg_version: bool,

    #[serde(default, skip_serializing_if = "is_default")]
    #[arg(long = "rga-profile")]
    pub profile: bool,

    #[serde(skip)]
    #[arg(long = "rga-doctor")]
    pub doctor: bool,

    #[serde(skip)]
    #[arg(long = "rga-max-rss-bytes", require_equals = true)]
    pub max_rss_bytes: Option<usize>,

    #[serde(skip)]
    #[arg(long = "rga-max-file-bytes", require_equals = true)]
    pub max_file_bytes: Option<usize>,

    #[serde(skip)]
    #[arg(long = "rga-cache-prune")]
    pub cache_prune: bool,

    #[serde(skip)]
    #[arg(long = "rga-cache-prune-max-bytes", require_equals = true)]
    pub cache_prune_max_bytes: Option<String>,

    #[serde(skip)]
    #[arg(long = "rga-cache-prune-ttl-days", require_equals = true)]
    pub cache_prune_ttl_days: Option<i32>,

    /// Disable page break postprocessing (removes "Page N:" prefixes)
    #[serde(default, skip_serializing_if = "is_default")]
    #[arg(long = "rga-disable-pagebreaks")]
    pub disable_pagebreaks: bool,

    #[serde(default, skip_serializing_if = "is_default")]
    #[arg(long = "rga-ipynb-include-outputs")]
    pub ipynb_include_outputs: bool,

    /// Disable line prefixing for specific adapters (comma-delimited names)
    #[serde(default, skip_serializing_if = "is_default")]
    #[arg(long = "rga-no-prefix-for", require_equals = true, value_delimiter = ',')]
    pub no_prefix_for_adapters: Vec<String>,
}

#[derive(Parser, Debug, Deserialize, Serialize, JsonSchema, Default, Clone, PartialEq)]
pub struct CacheConfig {
    /// Disable caching of results.
    ///
    /// By default, rga caches the extracted text, if it is small enough, to a database.
    /// This way, repeated searches on the same set of files will be much faster.
    /// The location of the DB varies by platform:
    /// - `${XDG_CACHE_DIR-~/.cache}/ripgrep-all` on Linux
    /// - `~/Library/Caches/ripgrep-all` on macOS
    /// - `C:\Users\username\AppData\Local\ripgrep-all` on Windows
    ///
    /// If you pass this flag, all caching will be disabled.
    #[serde(default, skip_serializing_if = "is_default")]
    #[arg(long = "rga-no-cache")]
    pub disabled: bool,

    /// Max compressed size to cache.
    ///
    /// Longest byte length (after compression) to store in cache.
    /// Longer adapter outputs will not be cached and recomputed every time.
    ///
    /// Allowed suffixes on command line: k M G
    #[serde(default, skip_serializing_if = "is_default")]
    #[arg(
        default_value_t,
        long = "rga-cache-max-blob-len",
        hide_short_help = true,
        require_equals = true,
        // parse(try_from_str = parse_readable_bytes_str)
    )]
    pub max_blob_len: CacheMaxBlobLen,

    /// ZSTD compression level to apply to adapter outputs before storing in cache DB.
    ///
    /// Ranges from 1 - 22.
    #[serde(default, skip_serializing_if = "is_default")]
    #[arg(
        default_value_t,
        long = "rga-cache-compression-level",
        hide_short_help = true,
        require_equals = true,
        help = ""
    )]
    pub compression_level: CacheCompressionLevel,

    /// Path to store cache DB.
    #[serde(default, skip_serializing_if = "is_default")]
    #[arg(
        default_value_t,
        long = "rga-cache-path",
        hide_short_help = true,
        require_equals = true
    )]
    pub path: CachePath,

    /// Zstd dictionary path for cache compression warm start
    #[serde(default, skip_serializing_if = "is_default")]
    #[arg(long = "rga-cache-dict-path", require_equals = true)]
    pub dict_path: Option<String>,

    #[serde(default, skip_serializing_if = "is_default")]
    #[arg(
        default_value_t,
        long = "rga-cache-small-uncompressed",
        hide_short_help = true,
        require_equals = true
    )]
    pub small_uncompressed_bytes: CacheSmallUncompressedBytes,

    #[serde(default, skip_serializing_if = "is_default")]
    #[arg(long = "rga-cache-no-small-uncompressed")]
    pub disable_small_uncompressed: bool,
}

static RGA_CONFIG: &str = "RGA_CONFIG";

use serde_json::Value;
fn json_merge(a: &mut Value, b: &Value) {
    match (a, b) {
        (&mut Value::Object(ref mut a), Value::Object(b)) => {
            for (k, v) in b {
                json_merge(a.entry(k.clone()).or_insert(Value::Null), v);
            }
        }
        (a, b) => {
            *a = b.clone();
        }
    }
}

fn read_config_file(path_override: Option<String>) -> Result<(String, Value)> {
    let proj = project_dirs()?;
    let config_dir = proj.config_dir();
    let config_filename = path_override
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| config_dir.join("config.json"));
    let config_filename_str = config_filename.to_string_lossy().into_owned();
    if config_filename.exists() {
        let config_file_contents = {
            let raw = std::fs::read_to_string(config_filename).with_context(|| {
                format!("Could not read config file json {config_filename_str}")
            })?;
            let mut s = String::new();
            json_comments::StripComments::new(raw.as_bytes())
                .read_to_string(&mut s)
                .context("strip comments")?;
            s
        };
        {
            // just for error messages, actual deserialization happens after merging with cmd args
            serde_json::from_str::<RgaConfig>(&config_file_contents).with_context(|| {
                format!("Error in config file {config_filename_str}: {config_file_contents}")
            })?;
        }
        let config_json: serde_json::Value =
            serde_json::from_str(&config_file_contents).context("Could not parse config json")?;
        Ok((config_filename_str, config_json))
    } else if let Some(p) = path_override.as_ref() {
        Err(anyhow::anyhow!("Config file not found: {}", p))?
    } else {
        // write default config
        std::fs::create_dir_all(config_dir)?;
        let mut schemafile = File::create(config_dir.join("config.v1.schema.json"))?;

        schemafile.write_all(
            serde_json::to_string_pretty(&schemars::schema_for!(RgaConfig))?.as_bytes(),
        )?;

        let mut configfile = File::create(config_filename)?;
        configfile.write_all(include_str!("../doc/config.default.jsonc").as_bytes())?;
        Ok((
            config_filename_str,
            serde_json::Value::Object(Default::default()),
        ))
    }
}
fn read_config_env() -> Result<Value> {
    let val = std::env::var(RGA_CONFIG).ok();
    if let Some(val) = val {
        serde_json::from_str(&val).context("could not parse config from env RGA_CONFIG")
    } else {
        serde_json::to_value(RgaConfig::default()).context("could not create default config")
    }
}
pub fn parse_args<I>(args: I, is_rga_preproc: bool) -> Result<RgaConfig>
where
    I: IntoIterator,
    I::Item: Into<OsString> + Clone,
{
    // TODO: don't read config file in rga-preproc for performance (called for every file)

    let arg_matches: RgaConfig = RgaConfig::parse_from(args);
    let args_config = serde_json::to_value(&arg_matches)?;

    let merged_config = {
        if is_rga_preproc {
            // only read from env and args
            let mut merged_config = read_config_env()?;
            json_merge(&mut merged_config, &args_config);
            log::debug!("Config: {}", serde_json::to_string(&merged_config)?);
            merged_config
        } else {
            // read from config file, env and args
            let (config_filename, config_file_config) =
                read_config_file(arg_matches.config_file_path)?;
            let env_var_config = read_config_env()?;
            let mut merged_config = config_file_config.clone();
            json_merge(&mut merged_config, &env_var_config);
            json_merge(&mut merged_config, &args_config);
            log::debug!(
                "Configs:\n{}: {}\n{}: {}\nArgs: {}\nMerged: {}",
                config_filename,
                serde_json::to_string_pretty(&config_file_config)?,
                RGA_CONFIG,
                serde_json::to_string_pretty(&env_var_config)?,
                serde_json::to_string_pretty(&args_config)?,
                serde_json::to_string_pretty(&merged_config)?
            );
            // pass to child processes
            // TODO: Audit that the environment access only happens in single-threaded code.
            unsafe { std::env::set_var(RGA_CONFIG, merged_config.to_string()) };
            merged_config
        }
    };

    let mut res: RgaConfig = serde_json::from_value(merged_config.clone())
        .map_err(|e| {
            println!("{e:?}");
            e
        })
        .with_context(|| {
            format!(
                "Error parsing merged config: {}",
                serde_json::to_string_pretty(&merged_config).expect("no tostring")
            )
        })?;
    {
        // readd values with [serde(skip)]
        res.fzf_path = arg_matches.fzf_path;
        res.list_adapters = arg_matches.list_adapters;
        res.print_config_schema = arg_matches.print_config_schema;
        res.rg_help = arg_matches.rg_help;
        res.rg_version = arg_matches.rg_version;
        let auto_sweep_path = if arg_matches.load_sweep_path.is_some() {
            arg_matches.load_sweep_path.clone()
        } else {
            project_dirs().ok().and_then(|pd| {
                // Prefer autoconfig.txt; fall back to sweep-results.toml
                let rec = pd.cache_dir().join("autoconfig.txt");
                if std::fs::metadata(&rec).is_ok() { return Some(rec.to_string_lossy().to_string()); }
                let p = pd.cache_dir().join("sweep-results.toml");
                if std::fs::metadata(&p).is_ok() { Some(p.to_string_lossy().to_string()) } else { None }
            })
        };
        let auto_sweep_path = if auto_sweep_path.is_some() { auto_sweep_path } else {
            project_dirs().ok().and_then(|pd| {
                let rec = pd.cache_dir().join("autoconfig.txt");
                if std::fs::metadata(&rec).is_err()
                    && let Ok(exe) = std::env::current_exe()
                    && let Some(dir) = exe.parent()
                {
                    let cand = if cfg!(windows) { dir.join("bench-sweep.exe") } else { dir.join("bench-sweep") };
                    if std::fs::metadata(&cand).is_ok() {
                        let _ = std::fs::create_dir_all(pd.cache_dir());
                        let _ = std::process::Command::new(cand).args(["--out-txt", rec.to_string_lossy().as_ref()]).status();
                    }
                }
                if std::fs::metadata(&rec).is_ok() { Some(rec.to_string_lossy().to_string()) } else { None }
            })
        };
        if let Some(path) = auto_sweep_path.as_ref() {
            if path.ends_with(".toml") {
                #[derive(DeDeserialize)]
                struct ZipSec { best_concurrency: usize, best_pipe_bytes: usize }
                #[derive(DeDeserialize)]
                struct ZipModeSec { owned_iter: bool }
                #[derive(DeDeserialize)]
                struct PostprocSec { best_pipe_bytes: usize }
                #[derive(DeDeserialize)]
                struct BufSec { best_buf_bytes: usize }
                #[derive(DeDeserialize)]
                struct ResultsToml { zip: ZipSec, zip_mode: ZipModeSec, postproc: PostprocSec, zstd: BufSec, gzip: BufSec, xz: BufSec, bzip2: BufSec }
                let txt = std::fs::read_to_string(path).with_context(|| format!("reading sweep toml {path}"))?;
                let r: ResultsToml = toml::from_str(&txt).with_context(|| "parsing sweep toml" )?;
                res.zip_max_concurrency = ZipMaxConcurrency(r.zip.best_concurrency);
                res.zip_pipe_bytes = ZipPipeBytes(r.zip.best_pipe_bytes);
                res.zip_owned_iter = r.zip_mode.owned_iter;
                res.postproc_pipe_bytes = PostprocPipeBytes(r.postproc.best_pipe_bytes);
                res.decompress_zstd_buf_bytes = DecompressZstdBufBytes(r.zstd.best_buf_bytes);
                res.decompress_gzip_buf_bytes = DecompressGzipBufBytes(r.gzip.best_buf_bytes);
                res.decompress_xz_buf_bytes = DecompressXzBufBytes(r.xz.best_buf_bytes);
                res.decompress_bzip2_buf_bytes = DecompressBzip2BufBytes(r.bzip2.best_buf_bytes);
            } else {
                let txt = std::fs::read_to_string(path).with_context(|| format!("reading sweep txt {path}"))?;
                for tok in txt.split(|c: char| [' ', ',', '\n'].contains(&c)) {
                    if let Some((k, v)) = tok.split_once('=') {
                        match k {
                            "rga-zip-max-concurrency" => if let Ok(u) = v.parse::<usize>() { res.zip_max_concurrency = ZipMaxConcurrency(u); },
                            "rga-zip-pipe-bytes" => if let Ok(u) = v.parse::<usize>() { res.zip_pipe_bytes = ZipPipeBytes(u); },
                            "rga-zip-owned-iter" => if let Ok(b) = v.parse::<bool>() { res.zip_owned_iter = b; },
                            "rga-postproc-pipe-bytes" => if let Ok(u) = v.parse::<usize>() { res.postproc_pipe_bytes = PostprocPipeBytes(u); },
                            "rga-decompress-zstd-buf-bytes" => if let Ok(u) = v.parse::<usize>() { res.decompress_zstd_buf_bytes = DecompressZstdBufBytes(u); },
                            "rga-decompress-gzip-buf-bytes" => if let Ok(u) = v.parse::<usize>() { res.decompress_gzip_buf_bytes = DecompressGzipBufBytes(u); },
                            "rga-decompress-xz-buf-bytes" => if let Ok(u) = v.parse::<usize>() { res.decompress_xz_buf_bytes = DecompressXzBufBytes(u); },
                            "rga-decompress-bzip2-buf-bytes" => if let Ok(u) = v.parse::<usize>() { res.decompress_bzip2_buf_bytes = DecompressBzip2BufBytes(u); },
                            _ => {}
                        }
                    }
                }
            }
        }
    }
    Ok(res)
}

/// Split arguments into the ones we care about and the ones rg cares about
pub fn split_args(is_rga_preproc: bool) -> Result<(RgaConfig, Vec<OsString>)> {
    let mut firstarg = true;
    // debug!("{:#?}", app.p.flags);
    let (our_args, mut passthrough_args): (Vec<OsString>, Vec<OsString>) = std::env::args_os()
        .partition(|os_arg| {
            if firstarg {
                // hacky, but .enumerate() would be ugly because partition is too simplistic
                firstarg = false;
                return true;
            }
            if let Some(arg) = os_arg.to_str() {
                arg.starts_with("--rga-")
                    || arg.starts_with("--rg-")
                    || arg == "--help"
                    || arg == "-h"
                    || arg == "--version"
                    || arg == "-V"
            } else {
                // args that are not unicode can only be filenames, pass them to rg
                false
            }
        });
    debug!("rga (our) args: {:?}", our_args);
    let matches = parse_args(our_args, is_rga_preproc).context("Could not parse config")?;
    if matches.rg_help {
        passthrough_args.insert(0, "--help".into());
    }
    if matches.rg_version {
        passthrough_args.insert(0, "--version".into());
    }
    debug!("rga (passthrough) args: {:?}", passthrough_args);
    Ok((matches, passthrough_args))
}
