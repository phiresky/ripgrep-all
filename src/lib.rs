#![warn(clippy::all)]

pub mod adapters;
pub mod args;
mod caching_writer;
pub mod matching;
pub mod preproc;
pub mod preproc_cache;
use anyhow::Context;
use anyhow::Result;
pub use caching_writer::CachingWriter;
use directories_next::ProjectDirs;

pub fn project_dirs() -> Result<ProjectDirs> {
    directories_next::ProjectDirs::from("", "", "ripgrep-all")
        .context("no home directory found! :(")
}
