
use failure::Fallible;
use log::*;
use serde::{Deserialize, Serialize};

use std::ffi::OsString;
use std::iter::IntoIterator;

use structopt::StructOpt;
fn is_default<T: Default + PartialEq>(t: &T) -> bool {
    t == &T::default()
}

#[derive(StructOpt, Debug, Deserialize, Serialize)]
#[structopt(rename_all = "kebab-case")]
pub struct RgaOptions {
    #[serde(default, skip_serializing_if = "is_default")]
    #[structopt(long, help = "Disable caching of results")]
    pub no_cache: bool,

    #[serde(default, skip_serializing_if = "is_default")]
    #[structopt(
        long,
        require_equals = true,
        require_delimiter = true,
        help = "Change which adapters to use and in which priority order (descending)"
    )]
    pub adapters: Vec<String>,

    #[serde(skip)]
    #[structopt(long, help = "Show help for ripgrep itself")]
    pub rg_help: bool,

    #[serde(skip)]
    #[structopt(long, help = "Show version of ripgrep itself")]
    pub rg_version: bool,

    #[serde(skip)]
    #[structopt(long, help = "List all known adapters")]
    pub list_adapters: bool,
}

static RGA_CONFIG: &str = "RGA_CONFIG";

pub fn parse_args<I>(args: I) -> Fallible<RgaOptions>
where
    I: IntoIterator,
    I::Item: Into<OsString> + Clone,
{
    match std::env::var(RGA_CONFIG) {
        Ok(val) => {
            error!("Loading args from env {}={}", RGA_CONFIG, val);
            Ok(serde_json::from_str(&val)?)
        }
        Err(_) => {
            let matches = RgaOptions::from_iter(args);
            let serialized_config = serde_json::to_string(&matches)?;
            std::env::set_var(RGA_CONFIG, &serialized_config);
            debug!("{}={}", RGA_CONFIG, serialized_config);

            Ok(matches)
        }
    }
}
