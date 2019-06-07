#![warn(clippy::all)]

pub mod adapters;
pub mod args;
mod caching_writer;
pub mod preproc;
pub mod preproc_cache;
pub use caching_writer::CachingWriter;
