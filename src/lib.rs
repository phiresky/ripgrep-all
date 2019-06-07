#![warn(clippy::all)]

pub mod adapters;
mod caching_writer;
pub mod preproc;
pub mod preproc_cache;
pub use caching_writer::CachingWriter;
