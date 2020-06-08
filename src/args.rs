use crate::project_dirs;
use anyhow::*;
use derive_more::FromStr;
use log::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::{fs::File, io::Write, iter::IntoIterator, str::FromStr};
use structopt::StructOpt;

#[derive(Debug, Deserialize, Serialize)]
struct ReadableBytesCount(i64);

fn is_default<T: Default + PartialEq>(t: &T) -> bool {
    t == &T::default()
}
#[derive(JsonSchema, Debug, Serialize, Deserialize, Copy, Clone, PartialEq, FromStr)]
pub struct CacheCompressionLevel(pub i32);

impl ToString for CacheCompressionLevel {
    fn to_string(&self) -> String {
        self.0.to_string()
    }
}
impl Default for CacheCompressionLevel {
    fn default() -> Self {
        CacheCompressionLevel(12)
    }
}
#[derive(JsonSchema, Debug, Serialize, Deserialize, Copy, Clone, PartialEq, FromStr)]
pub struct MaxArchiveRecursion(pub i32);

impl ToString for MaxArchiveRecursion {
    fn to_string(&self) -> String {
        self.0.to_string()
    }
}
impl Default for MaxArchiveRecursion {
    fn default() -> Self {
        MaxArchiveRecursion(4)
    }
}

#[derive(JsonSchema, Debug, Serialize, Deserialize, Copy, Clone, PartialEq)]
pub struct CacheMaxBlobLen(pub usize);

impl ToString for CacheMaxBlobLen {
    fn to_string(&self) -> String {
        self.0.to_string()
    }
}
impl Default for CacheMaxBlobLen {
    fn default() -> Self {
        CacheMaxBlobLen(2000000)
    }
}

impl FromStr for CacheMaxBlobLen {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let suffix = s.chars().last();
        if let Some(suffix) = suffix {
            Ok(CacheMaxBlobLen(match suffix {
                'k' | 'M' | 'G' => usize::from_str(s.trim_end_matches(suffix))
                    .with_context(|| format!("Could not parse int"))
                    .map(|e| {
                        e * match suffix {
                            'k' => 1000,
                            'M' => 1000_000,
                            'G' => 1000_000_000,
                            _ => panic!("impossible"),
                        }
                    }),
                _ => usize::from_str(s).with_context(|| format!("Could not parse int")),
            }?))
        } else {
            Err(format_err!("empty byte input"))
        }
    }
}

#[derive(StructOpt, Debug, Deserialize, Serialize, JsonSchema, Default)]
#[structopt(
    name = "ripgrep-all",
    rename_all = "kebab-case",
    about = env!("CARGO_PKG_DESCRIPTION"),
    author = env!("CARGO_PKG_HOMEPAGE"),
    // TODO: long_about does not seem to work to only show this on short help
    after_help = "-h shows a concise overview, --help shows more detail and advanced options.\n\nAll other options not shown here are passed directly to rg, especially [PATTERN] and [PATH ...]",
    usage = "rga [RGA OPTIONS] [RG OPTIONS] PATTERN [PATH ...]"
)]

/// # rga configuration
///
/// this is kind of a "polyglot" struct, since it serves three functions
///
/// 1. describing the command line arguments using structopt+clap
/// 2. describing the config file format (output as JSON schema via schemars)
pub struct RgaConfig {
    #[serde(default, skip_serializing_if = "is_default")]
    #[structopt(long = "--rga-no-cache")]
    /// Disable caching of results
    ///
    /// By default, rga caches the extracted text, if it is small enough,
    /// to a database in ~/.cache/rga on Linux,
    /// ~/Library/Caches/rga on macOS,
    /// or C:\Users\username\AppData\Local\rga on Windows.
    /// This way, repeated searches on the same set of files will be much faster.
    /// If you pass this flag, all caching will be disabled.
    pub no_cache: bool,

    #[serde(default, skip_serializing_if = "is_default")]
    #[structopt(long = "--rga-accurate")]
    /// Use more accurate but slower matching by mime type
    ///
    /// By default, rga will match files using file extensions.
    /// Some programs, such as sqlite3, don't care about the file extension at all,
    /// so users sometimes use any or no extension at all. With this flag, rga
    /// will try to detect the mime type of input files using the magic bytes
    /// (similar to the `file` utility), and use that to choose the adapter.
    /// Detection is only done on the first 8KiB of the file, since we can't always seek on the input (in archives).
    pub accurate: bool,

    #[serde(default, skip_serializing_if = "is_default")]
    #[structopt(
        long = "--rga-adapters",
        require_equals = true,
        require_delimiter = true
    )]
    /// Change which adapters to use and in which priority order (descending)
    ///
    /// "foo,bar" means use only adapters foo and bar.
    /// "-bar,baz" means use all default adapters except for bar and baz.
    /// "+bar,baz" means use all default adapters and also bar and baz.
    pub adapters: Vec<String>,

    #[serde(default, skip_serializing_if = "is_default")]
    #[structopt(
        default_value,
        long = "--rga-cache-max-blob-len",
        hidden_short_help = true,
        require_equals = true,
        // parse(try_from_str = parse_readable_bytes_str)
    )]
    /// Max compressed size to cache
    ///
    /// Longest byte length (after compression) to store in cache. Longer adapter outputs will not be cached and recomputed every time. Allowed suffixes: k M G
    pub cache_max_blob_len: CacheMaxBlobLen,

    #[serde(default, skip_serializing_if = "is_default")]
    #[structopt(
        default_value,
        long = "--rga-cache-compression-level",
        hidden_short_help = true,
        require_equals = true,
        help = ""
    )]
    /// ZSTD compression level to apply to adapter outputs before storing in cache db
    ///
    ///  Ranges from 1 - 22
    pub cache_compression_level: CacheCompressionLevel,

    #[serde(default, skip_serializing_if = "is_default")]
    #[structopt(
        default_value,
        long = "--rga-max-archive-recursion",
        require_equals = true,
        hidden_short_help = true
    )]
    /// Maximum nestedness of archives to recurse into
    pub max_archive_recursion: MaxArchiveRecursion,

    #[serde(skip)]
    #[structopt(long = "--rga-fzf-path", require_equals = true, hidden = true)]
    /// same as passing path directly, except if argument is empty
    /// kinda hacky, but if no file is found, fzf calls rga with empty string as path, which causes No such file or directory from rg. So filter those cases and return specially
    pub fzf_path: Option<String>,

    // these arguments are basically "subcommands" that stop the process, so don't serialize them
    #[serde(skip)]
    #[structopt(long = "--rga-list-adapters", help = "List all known adapters")]
    pub list_adapters: bool,

    #[serde(skip)]
    #[structopt(
        long = "--rga-print-config-schema",
        help = "Print the JSON Schema of the configuration file"
    )]
    pub print_config_schema: bool,

    #[serde(skip)]
    #[structopt(long, help = "Show help for ripgrep itself")]
    pub rg_help: bool,

    #[serde(skip)]
    #[structopt(long, help = "Show version of ripgrep itself")]
    pub rg_version: bool,

    #[serde(rename = "$schema", default = "default_schema_path")]
    #[structopt(skip)]
    pub _schema_key: String,
}
fn default_schema_path() -> String {
    "./config.schema.json".to_string()
}

static RGA_CONFIG: &str = "RGA_CONFIG";

pub fn parse_args<I>(args: I) -> Result<RgaConfig>
where
    I: IntoIterator,
    I::Item: Into<OsString> + Clone,
{
    let proj = project_dirs()?;
    let config_dir = proj.config_dir();
    if config_dir.join("config.json").exists() {
        // todo: read config
    } else {
        std::fs::create_dir_all(config_dir)?;
        let mut schemafile = File::create(config_dir.join("config.schema.json"))?;

        schemafile
            .write(serde_json::to_string_pretty(&schemars::schema_for!(RgaConfig))?.as_bytes())?;

        let mut configfile = File::create(config_dir.join("config.json"))?;
        let mut v = serde_json::to_value(&RgaConfig::default())?;
        match &mut v {
            serde_json::Value::Object(o) => {
                o["$schema"] = serde_json::Value::String("./config.schema.json".to_string())
            }
            _ => panic!("impos"),
        }
        configfile.write(serde_json::to_string_pretty(&v)?.as_bytes())?;
    }
    match std::env::var(RGA_CONFIG) {
        Ok(val) => {
            debug!(
                "Loading args from env {}={}, ignoring cmd args",
                RGA_CONFIG, val
            );
            Ok(serde_json::from_str(&val)?)
        }
        Err(_) => {
            let matches = RgaConfig::from_iter(args);
            let serialized_config = serde_json::to_string(&matches)?;
            std::env::set_var(RGA_CONFIG, &serialized_config);
            debug!("{}={}", RGA_CONFIG, serialized_config);

            Ok(matches)
        }
    }
}

/// Split arguments into the ones we care about and the ones rg cares about
pub fn split_args() -> Result<(RgaConfig, Vec<OsString>)> {
    let mut app = RgaConfig::clap();

    app.p.create_help_and_version();
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
    debug!("our_args: {:?}", our_args);
    let matches = parse_args(our_args)?;
    if matches.rg_help {
        passthrough_args.insert(0, "--help".into());
    }
    if matches.rg_version {
        passthrough_args.insert(0, "--version".into());
    }
    debug!("passthrough_args: {:?}", passthrough_args);
    Ok((matches, passthrough_args))
}
