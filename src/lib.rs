#![warn(clippy::all)]

pub mod adapted_iter;
pub mod adapters;
mod caching_writer;
pub mod config;
pub mod expand;
pub mod matching;
pub mod preproc;
pub mod preproc_cache;
pub mod recurse;
#[cfg(test)]
pub mod test_utils;
use anyhow::Context;
use anyhow::Result;
use async_stream::stream;
use directories_next::ProjectDirs;
use std::time::Instant;
use tokio::io::AsyncRead;
use tokio::task::JoinHandle;
use tokio_util::io::StreamReader;

pub fn project_dirs() -> Result<ProjectDirs> {
    directories_next::ProjectDirs::from("", "", "ripgrep-all")
        .context("no home directory found! :(")
}

// no "significant digits" format specifier in rust??
// https://stackoverflow.com/questions/60497397/how-do-you-format-a-float-to-the-first-significant-decimal-and-with-specified-pr
fn meh(float: f32, precision: usize) -> usize {
    // compute absolute value
    let a = float.abs();

    // if abs value is greater than 1, then precision becomes less than "standard"

    if a >= 1. {
        // reduce by number of digits, minimum 0
        let n = (1. + a.log10().floor()) as usize;
        precision.saturating_sub(n)
    // if precision is less than 1 (but non-zero), then precision becomes greater than "standard"
    } else if a > 0. {
        // increase number of digits
        let n = -(1. + a.log10().floor()) as usize;
        precision + n
    // special case for 0
    } else {
        0
    }
}

pub fn print_dur(start: Instant) -> String {
    let mut dur = Instant::now().duration_since(start).as_secs_f32();
    let mut suffix = "";
    if dur < 0.1 {
        suffix = "m";
        dur *= 1000.0;
    }
    let precision = meh(dur, 3);
    format!("{dur:.precision$}{suffix}s")
}

pub fn print_bytes(bytes: impl Into<f64>) -> String {
    pretty_bytes::converter::convert(bytes.into())
}

pub fn to_io_err(e: anyhow::Error) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Other, e)
}

#[cfg(test)]
#[ctor::ctor]
fn init() {
    env_logger::init();
}

/** returns an AsyncRead that is empty but returns an io error if the given task had an io error or join error */
pub fn join_handle_to_stream(join: JoinHandle<std::io::Result<()>>) -> impl AsyncRead {
    let st = stream! {
        join.await.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))??;
        yield std::io::Result::Ok(&b""[..])
    };

    StreamReader::new(st)
}
