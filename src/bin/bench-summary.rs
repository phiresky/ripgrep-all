use ripgrep_all::project_dirs;
use std::path::PathBuf;
use std::fs;
use serde::Deserialize;

#[derive(Deserialize)]
struct ZipSec { avg_secs: f64, peak_bytes: u64, best_concurrency: usize, best_pipe_bytes: usize }
#[derive(Deserialize)]
struct ZipModeSec { avg_secs: f64, peak_bytes: u64, owned_iter: bool }
#[derive(Deserialize)]
struct PostprocSec { avg_secs: f64, peak_bytes: u64, best_pipe_bytes: usize }
#[derive(Deserialize)]
struct BufSec { avg_secs: f64, peak_bytes: u64, best_buf_bytes: usize }
#[derive(Deserialize)]
struct MboxSec { avg_secs: f64, peak_bytes: u64 }

#[derive(Deserialize)]
struct ResultsToml {
    zip: ZipSec,
    zip_mode: ZipModeSec,
    postproc: PostprocSec,
    zstd: BufSec,
    gzip: BufSec,
    xz: BufSec,
    bzip2: BufSec,
    mbox: MboxSec,
}

fn default_path() -> PathBuf {
    let pd = project_dirs().expect("dirs");
    pd.cache_dir().join("sweep-results.toml")
}

fn parse_path_arg() -> PathBuf {
    for a in std::env::args().skip(1) {
        if let Some(p) = a.strip_prefix("--path=") { return PathBuf::from(p); }
    }
    default_path()
}

fn print_row(label: &str, avg: f64, peak: u64, param: &str) {
    println!("{:<10}  avg={:.3}s  peak={} bytes  {}", label, avg, peak, param);
}

fn main() -> anyhow::Result<()> {
    let path = parse_path_arg();
    let txt = fs::read_to_string(&path)?;
    let r: ResultsToml = toml::from_str(&txt)?;
    println!("Summary from {}", path.to_string_lossy());
    print_row("zip", r.zip.avg_secs, r.zip.peak_bytes, &format!("concurrency={}, pipe_bytes={}", r.zip.best_concurrency, r.zip.best_pipe_bytes));
    print_row("zip-mode", r.zip_mode.avg_secs, r.zip_mode.peak_bytes, &format!("owned_iter={}", r.zip_mode.owned_iter));
    print_row("postproc", r.postproc.avg_secs, r.postproc.peak_bytes, &format!("pipe_bytes={}", r.postproc.best_pipe_bytes));
    print_row("zstd", r.zstd.avg_secs, r.zstd.peak_bytes, &format!("buf_bytes={}", r.zstd.best_buf_bytes));
    print_row("gzip", r.gzip.avg_secs, r.gzip.peak_bytes, &format!("buf_bytes={}", r.gzip.best_buf_bytes));
    print_row("xz", r.xz.avg_secs, r.xz.peak_bytes, &format!("buf_bytes={}", r.xz.best_buf_bytes));
    print_row("bzip2", r.bzip2.avg_secs, r.bzip2.peak_bytes, &format!("buf_bytes={}", r.bzip2.best_buf_bytes));
    print_row("mbox", r.mbox.avg_secs, r.mbox.peak_bytes, "");
    Ok(())
}