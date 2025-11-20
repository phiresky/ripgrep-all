use ripgrep_all as rga;
use std::io::Write;
use std::time::Instant;
use tempfile::tempdir;
use tokio::fs::File;
use tokio::io::{copy, sink};

fn write_mbox(path: &std::path::Path, mb: usize) -> anyhow::Result<()> {
    let mut f = std::fs::File::create(path)?;
    // simple plain-text messages separated by mbox From lines
    // message body ~4 KiB to scale
    let body = {
        let mut s = String::new();
        for i in 0..128 { s.push_str(&format!("Line {i}: hello world lorem ipsum dolor sit amet\n")); }
        s
    };
    let mut written = 0usize;
    let target = mb * 1024 * 1024;
    let mut idx = 0usize;
    while written < target {
        let msg = format!(
            "From sender@example.com Sun Jan 01 00:00:00 2024\nSubject: Test {idx}\nContent-Type: text/plain; charset=utf-8\n\n{body}\n"
        );
        f.write_all(msg.as_bytes())?;
        written = written.saturating_add(msg.len());
        idx = idx.saturating_add(1);
    }
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let path = dir.path().join("bench.mbox");
    let mut mb = 100usize;
    let mut csv: Option<String> = None;
    for arg in std::env::args().skip(1) {
        if let Some(v) = arg.strip_prefix("--mb=") { mb = v.parse().unwrap_or(mb); }
        if let Some(v) = arg.strip_prefix("--csv=") { csv = Some(v.to_string()); }
    }
    write_mbox(&path, mb)?;
    let cfg = rga::config::RgaConfig { accurate: true, cache: rga::config::CacheConfig { disabled: true, ..Default::default() }, ..Default::default() };
    let i = File::open(&path).await?;
    let ai = rga::adapters::AdaptInfo {
        inp: Box::pin(i),
        filepath_hint: path.clone(),
        is_real_file: true,
        line_prefix: "".to_string(),
        archive_recursion_depth: 0,
        postprocess: true,
        config: cfg,
    };
    let start = Instant::now();
    let mut rd = rga::preproc::rga_preproc(ai).await?;
    let mut w = sink();
    let copied = copy(&mut rd, &mut w).await?;
    let dur = rga::print_dur(start);
    println!("mbox: {} bytes={} mb={}", dur, copied, mb);
    if let Some(csvp) = csv {
        let exists = std::path::Path::new(&csvp).exists();
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(csvp)?;
        if !exists { writeln!(f, "bench,bytes,duration_ms,extra")?; }
        let ms = (std::time::Instant::now() - start).as_millis();
        writeln!(f, "mbox,{},{},mb:{}", copied, ms, mb)?;
    }
    Ok(())
}