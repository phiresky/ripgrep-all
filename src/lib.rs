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
use std::time::Instant;

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
    let precision = if a >= 1. {
        // reduce by number of digits, minimum 0
        let n = (1. + a.log10().floor()) as usize;
        if n <= precision {
            precision - n
        } else {
            0
        }
    // if precision is less than 1 (but non-zero), then precision becomes greater than "standard"
    } else if a > 0. {
        // increase number of digits
        let n = -(1. + a.log10().floor()) as usize;
        precision + n
    // special case for 0
    } else {
        0
    };
    precision
}

pub fn print_dur(start: Instant) -> String {
    let mut dur = Instant::now().duration_since(start).as_secs_f32();
    let mut suffix = "";
    if dur < 0.1 {
        suffix = "m";
        dur *= 1000.0;
    }
    let precision = meh(dur, 3);
    format!(
        "{dur:.prec$}{suffix}s",
        dur = dur,
        prec = precision,
        suffix = suffix
    )
}

pub fn print_bytes(bytes: impl Into<f64>) -> String {
    return pretty_bytes::converter::convert(bytes.into());
}
