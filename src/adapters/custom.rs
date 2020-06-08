use super::{spawning::SpawningFileAdapter, AdapterMeta, GetMetadata};
use crate::{
    matching::{FastMatcher, SlowMatcher},
    project_dirs,
};
use anyhow::*;
use derive_more::FromStr;
use log::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::{fs::File, io::Write, iter::IntoIterator, str::FromStr};
use structopt::StructOpt;

// mostly the same as AdapterMeta + SpawningFileAdapter
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default, PartialEq, Clone)]
pub struct CustomAdapterConfig {
    /// the unique identifier and name of this adapter. Must only include a-z, 0-9, _
    pub name: String,
    /// a description of this adapter. shown in help
    pub description: String,
    /// if true, the adapter will be disabled by default
    pub default_disabled: Option<bool>,
    /// version identifier. used to key cache entries, change if the configuration or program changes
    pub version: i32,
    /// the file extensions this adapter supports. For example ["epub", "mobi"]
    pub extensions: Vec<String>,
    /// if not null and --rga-accurate is enabled, mime type matching is used instead of file name matching
    pub mimetypes: Option<Vec<String>>,
    /// the name or path of the binary to run
    pub binary: String,
    /// The arguments to run the program with. Placeholders:
    /// {}: the file path (TODO)
    /// stdin of the program will be connected to the input file, and stdout is assumed to be the converted file
    pub args: Vec<String>,
}

pub struct CustomSpawningFileAdapter {
    binary: String,
    args: Vec<String>,
    meta: AdapterMeta,
}
impl GetMetadata for CustomSpawningFileAdapter {
    fn metadata(&self) -> &AdapterMeta {
        &self.meta
    }
}
impl SpawningFileAdapter for CustomSpawningFileAdapter {
    fn get_exe(&self) -> &str {
        &self.binary
    }
    fn command(
        &self,
        filepath_hint: &std::path::Path,
        mut command: std::process::Command,
    ) -> std::process::Command {
        command.args(&self.args);
        command
    }
}
impl CustomAdapterConfig {
    pub fn to_adapter(self) -> CustomSpawningFileAdapter {
        CustomSpawningFileAdapter {
            binary: self.binary.clone(),
            args: self.args.clone(),
            meta: AdapterMeta {
                name: self.name,
                version: self.version,
                description: format!(
                    "{}\nRuns: {} {}",
                    self.description,
                    self.binary,
                    self.args.join(" ")
                ),
                recurses: false,
                fast_matchers: self
                    .extensions
                    .iter()
                    .map(|s| FastMatcher::FileExtension(s.to_string()))
                    .collect(),
                slow_matchers: self.mimetypes.map(|mimetypes| {
                    mimetypes
                        .iter()
                        .map(|s| SlowMatcher::MimeType(s.to_string()))
                        .collect()
                }),
            },
        }
    }
}
