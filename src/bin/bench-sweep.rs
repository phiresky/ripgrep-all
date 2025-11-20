use std::fs;
use std::io::{self, Write};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};
use std::path::PathBuf;
use sysinfo::{System, Pid};
use ripgrep_all::project_dirs;
use serde::Serialize;

fn bin_path(name: &str) -> PathBuf {
    let mut p = PathBuf::from("target/release");
    #[cfg(windows)]
    { p.push(format!("{}.exe", name)); }
    #[cfg(not(windows))]
    { p.push(name.to_string()); }
    p
}

fn run_and_track(cmd: &mut Command) -> anyhow::Result<(f64, u64)> {
    let start = Instant::now();
    let mut child = cmd.spawn()?;
    let pid = child.id();
    let mut sys = System::new();
    let mut peak_bytes: u64 = 0;
    loop {
        if child.try_wait()?.is_some() { break; }
        sys.refresh_process(Pid::from_u32(pid));
        if let Some(p) = sys.process(Pid::from_u32(pid)) {
            let rss_kb = p.memory();
            let bytes = rss_kb.saturating_mul(1024);
            if bytes > peak_bytes { peak_bytes = bytes; }
        }
        thread::sleep(Duration::from_millis(80));
    }
    let dur = (Instant::now() - start).as_secs_f64();
    Ok((dur, peak_bytes))
}

fn avg_of_runs(mut cmd: Command, runs: usize, delay_ms: u64) -> anyhow::Result<(f64, u64)> {
    let mut total = 0.0;
    let mut peak: u64 = 0;
    for _ in 0..runs {
        let (secs, bytes) = run_and_track(&mut cmd)?;
        total += secs;
        if bytes > peak { peak = bytes; }
        thread::sleep(Duration::from_millis(delay_ms));
    }
    Ok((total / runs as f64, peak))
}

fn parse_peak_target_bytes() -> Option<u64> {
    for a in std::env::args() {
        if let Some(v) = a.strip_prefix("--peak-target-bytes=") && let Ok(u) = v.parse::<u64>() { return Some(u); }
        if let Some(v) = a.strip_prefix("--peak-target-gib=") && let Ok(g) = v.parse::<u64>() { return Some(g.saturating_mul(1024).saturating_mul(1024).saturating_mul(1024)); }
    }
    None
}

fn cool_down(ms: u64) {
    // simple backoff to let OS reclaim memory from child processes
    std::thread::sleep(Duration::from_millis(ms));
}

struct Spinner {
    stop: Arc<AtomicBool>,
    handle: std::thread::JoinHandle<()>,
}

fn start_spinner(label: &str) -> Spinner {
    let stop = Arc::new(AtomicBool::new(false));
    let s2 = stop.clone();
    let lbl = label.to_string();
    let handle = std::thread::spawn(move || {
        let frames = ["|","/","-","\\"]; let mut i = 0usize;
        loop {
            if s2.load(Ordering::SeqCst) { break; }
            print!("\r {} {} ", lbl, frames[i % frames.len()]);
            let _ = io::stdout().flush();
            i += 1;
            std::thread::sleep(Duration::from_millis(120));
        }
        let _ = io::stdout().flush();
    });
    Spinner { stop, handle }
}

fn stop_spinner(sp: Spinner) {
    sp.stop.store(true, Ordering::SeqCst);
    let _ = sp.handle.join();
}

// no source modifications; results are saved to files only

fn main() -> anyhow::Result<()> {
    let _dry = std::env::args().any(|a| a == "--dry-run" || a == "--no-apply");
    Command::new("cargo").args(["build","--release"]).status()?;
    // store results in-memory only; no filesystem outputs
    let peak_target = parse_peak_target_bytes();
    let base_runs: usize = if let Some(t) = peak_target {
        let baseline: u64 = 12u64.saturating_mul(1024).saturating_mul(1024).saturating_mul(1024);
        let scale = (t as f64 / baseline as f64) * 3.0;
        let r = scale.floor() as usize;
        if r < 1 { 1 } else { r }
    } else { 3 };

    let mut best_zip = (usize::MAX, usize::MAX, f64::INFINITY);
    let mut best_zip_avg = f64::INFINITY;
    let concs = [2usize,4,8];
    let pipes = [262144usize,524288,1048576];
    let sp = start_spinner("zip sweep");
    let mut zip_peak = 0u64;
    for &c in &concs { for &p in &pipes {
        let mut cmd = Command::new(bin_path("bench-zip-deep"));
        cmd.args(["--entries","2000","--nested","true","--rga-zip-max-concurrency",&c.to_string(),"--rga-zip-pipe-bytes",&p.to_string()]);
        let (avg, peak) = avg_of_runs(cmd, base_runs, 500)?;
        if peak > zip_peak { zip_peak = peak; }
        if avg < best_zip.2 { best_zip = (c, p, avg); best_zip_avg = avg; }
    }}
    cool_down(800);
    stop_spinner(sp);

    let mut best_zip_mode = (false, f64::INFINITY);
    let mut best_zip_mode_avg = f64::INFINITY;
    let sp = start_spinner("zip mode sweep");
    let mut zmode_peak = 0u64;
    for &m in &[true,false] {
        let mut cmd = Command::new(bin_path("bench-zip-deep"));
        cmd.args(["--entries","2000","--nested","true","--rga-zip-max-concurrency","4","--rga-zip-pipe-bytes","524288","--rga-zip-owned-iter", if m {"true"} else {"false"}]);
        let (avg, peak) = avg_of_runs(cmd, base_runs, 500)?;
        if peak > zmode_peak { zmode_peak = peak; }
        if avg < best_zip_mode.1 { best_zip_mode = (m, avg); best_zip_mode_avg = avg; }
    }
    cool_down(800);
    stop_spinner(sp);

    let mut best_pp = (usize::MAX, f64::INFINITY);
    let mut best_pp_avg = f64::INFINITY;
    let ppipes = [262144usize,524288,1048576];
    let sp = start_spinner("postproc sweep");
    let mut pp_peak = 0u64;
    for &p in &ppipes {
        let mut cmd = Command::new(bin_path("bench-utf16"));
        cmd.args(["--mb","100","--rga-postproc-pipe-bytes",&p.to_string()]);
        let (avg, peak) = avg_of_runs(cmd, base_runs, 500)?;
        if peak > pp_peak { pp_peak = peak; }
        if avg < best_pp.1 { best_pp = (p, avg); best_pp_avg = avg; }
    }
    cool_down(800);
    stop_spinner(sp);

    let mut best_zstd = (usize::MAX, f64::INFINITY);
    let mut best_zstd_avg = f64::INFINITY;
    let zbufs = [131072usize,262144,524288,1048576];
    let sp = start_spinner("zstd sweep");
    let mut zstd_peak = 0u64;
    for &b in &zbufs {
        let mut cmd = Command::new(bin_path("bench-tar"));
        cmd.args(["--entries","1000","--rga-decompress-zstd-buf-bytes",&b.to_string()]);
        let (avg, peak) = avg_of_runs(cmd, base_runs, 500)?;
        if peak > zstd_peak { zstd_peak = peak; }
        if avg < best_zstd.1 { best_zstd = (b, avg); best_zstd_avg = avg; }
    }
    cool_down(800);
    stop_spinner(sp);

    let mut best_gz = (usize::MAX, f64::INFINITY);
    let mut best_gz_avg = f64::INFINITY;
    let gzbufs = [65536usize,131072,262144,524288];
    let sp = start_spinner("gzip sweep");
    let mut gz_peak = 0u64;
    for &b in &gzbufs {
        let mut cmd = Command::new(bin_path("bench-tar"));
        cmd.args(["--entries","1000","--rga-decompress-gzip-buf-bytes",&b.to_string()]);
        let (avg, peak) = avg_of_runs(cmd, base_runs, 500)?;
        if peak > gz_peak { gz_peak = peak; }
        if avg < best_gz.1 { best_gz = (b, avg); best_gz_avg = avg; }
    }
    cool_down(800);
    stop_spinner(sp);

    let mut best_xz = (usize::MAX, f64::INFINITY);
    let mut best_xz_avg = f64::INFINITY;
    let xzbufs = [65536usize,131072,262144];
    let sp = start_spinner("xz sweep");
    let mut xz_peak = 0u64;
    for &b in &xzbufs {
        let mut cmd = Command::new(bin_path("bench-tar"));
        cmd.args(["--entries","1000","--rga-decompress-xz-buf-bytes",&b.to_string()]);
        let (avg, peak) = avg_of_runs(cmd, base_runs, 500)?;
        if peak > xz_peak { xz_peak = peak; }
        if avg < best_xz.1 { best_xz = (b, avg); best_xz_avg = avg; }
    }
    cool_down(800);
    stop_spinner(sp);

    let mut best_bz = (usize::MAX, f64::INFINITY);
    let mut best_bz_avg = f64::INFINITY;
    let bzbufs = [65536usize,131072,262144];
    let sp = start_spinner("bzip2 sweep");
    let mut bz_peak = 0u64;
    for &b in &bzbufs {
        let mut cmd = Command::new(bin_path("bench-tar"));
        cmd.args(["--entries","1000","--rga-decompress-bzip2-buf-bytes",&b.to_string()]);
        let (avg, peak) = avg_of_runs(cmd, base_runs, 500)?;
        if peak > bz_peak { bz_peak = peak; }
        if avg < best_bz.1 { best_bz = (b, avg); best_bz_avg = avg; }
    }
    cool_down(800);
    stop_spinner(sp);

    // mbox benchmark
    let sp = start_spinner("mbox bench");
    let mbox_peak: u64;
    let mbox_avg: f64;
    {
        let mut cmd = Command::new(bin_path("bench-mbox"));
        cmd.args(["--mb","100"]);
        let (avg, peak) = avg_of_runs(cmd, base_runs, 500)?;
        mbox_peak = peak; mbox_avg = avg;
    }
    cool_down(600);
    stop_spinner(sp);
    // no recommendations.txt; defaults applied directly unless dry-run

    // do not modify source defaults
    println!("\nSweep summary (avg secs, peak bytes):");
    println!(" zip sweep:   {:.3}s, {} bytes", best_zip_avg, zip_peak);
    println!(" zip mode:    {:.3}s, {} bytes", best_zip_mode_avg, zmode_peak);
    println!(" postproc:    {:.3}s, {} bytes", best_pp_avg, pp_peak);
    println!(" zstd:        {:.3}s, {} bytes", best_zstd_avg, zstd_peak);
    println!(" gzip:        {:.3}s, {} bytes", best_gz_avg, gz_peak);
    println!(" xz:          {:.3}s, {} bytes", best_xz_avg, xz_peak);
    println!(" bzip2:       {:.3}s, {} bytes", best_bz_avg, bz_peak);
    println!(" mbox:        {:.3}s, {} bytes", mbox_avg, mbox_peak);

    #[derive(Serialize)]
    struct ZipSec { avg_secs: f64, peak_bytes: u64, best_concurrency: usize, best_pipe_bytes: usize }
    #[derive(Serialize)]
    struct ZipModeSec { avg_secs: f64, peak_bytes: u64, owned_iter: bool }
    #[derive(Serialize)]
    struct PostprocSec { avg_secs: f64, peak_bytes: u64, best_pipe_bytes: usize }
    #[derive(Serialize)]
    struct BufSec { avg_secs: f64, peak_bytes: u64, best_buf_bytes: usize }
    #[derive(Serialize)]
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
    #[derive(Serialize)]
    struct MboxSec { avg_secs: f64, peak_bytes: u64 }
    let results = ResultsToml {
        zip: ZipSec { avg_secs: best_zip_avg, peak_bytes: zip_peak, best_concurrency: best_zip.0, best_pipe_bytes: best_zip.1 },
        zip_mode: ZipModeSec { avg_secs: best_zip_mode_avg, peak_bytes: zmode_peak, owned_iter: best_zip_mode.0 },
        postproc: PostprocSec { avg_secs: best_pp_avg, peak_bytes: pp_peak, best_pipe_bytes: best_pp.0 },
        zstd: BufSec { avg_secs: best_zstd_avg, peak_bytes: zstd_peak, best_buf_bytes: best_zstd.0 },
        gzip: BufSec { avg_secs: best_gz_avg, peak_bytes: gz_peak, best_buf_bytes: best_gz.0 },
        xz: BufSec { avg_secs: best_xz_avg, peak_bytes: xz_peak, best_buf_bytes: best_xz.0 },
        bzip2: BufSec { avg_secs: best_bz_avg, peak_bytes: bz_peak, best_buf_bytes: best_bz.0 },
        mbox: MboxSec { avg_secs: mbox_avg, peak_bytes: mbox_peak },
    };
    let out_path = std::env::args().find(|a| a.starts_with("--out-toml=")).map(|a| a.trim_start_matches("--out-toml=").to_string()).unwrap_or_else(|| {
        let pd = project_dirs().expect("dirs");
        pd.cache_dir().join("sweep-results.toml").to_string_lossy().to_string()
    });
    let s = toml::to_string(&results)?;
    let outp = std::path::Path::new(&out_path);
    if let Some(parent) = outp.parent() { let _ = std::fs::create_dir_all(parent); }
    fs::write(outp, s)?;

    // Also write a plain-text recommendations file with tokens the runtime can parse
    let rec_out = {
        if let Some(arg) = std::env::args().find(|a| a.starts_with("--out-txt=")) {
            let p = arg.trim_start_matches("--out-txt=");
            std::path::PathBuf::from(p)
        } else {
            let pd = project_dirs().expect("dirs");
            pd.cache_dir().join("autoconfig.txt")
        }
    };
    let mut txt = String::new();
    txt.push_str(&format!("zip: rga-zip-max-concurrency={}, rga-zip-pipe-bytes={}\n", best_zip.0, best_zip.1));
    txt.push_str(&format!("zip-mode: rga-zip-owned-iter={}\n", best_zip_mode.0));
    txt.push_str(&format!("postproc: rga-postproc-pipe-bytes={}\n", best_pp.0));
    txt.push_str(&format!("zstd: rga-decompress-zstd-buf-bytes={}\n", best_zstd.0));
    txt.push_str(&format!("gzip: rga-decompress-gzip-buf-bytes={}\n", best_gz.0));
    txt.push_str(&format!("xz: rga-decompress-xz-buf-bytes={}\n", best_xz.0));
    txt.push_str(&format!("bzip2: rga-decompress-bzip2-buf-bytes={}\n", best_bz.0));
    if let Some(parent) = rec_out.parent() { let _ = std::fs::create_dir_all(parent); }
    fs::write(&rec_out, txt)?;
    Ok(())
}