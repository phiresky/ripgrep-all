#![allow(unused_mut)]
use ripgrep_all as rga;
use std::io::Write;
use std::time::Instant;
use tempfile::tempdir;
use tokio::fs::File;
use tokio::io::{copy, sink};

fn write_utf16_le(path: &std::path::Path, mb: usize) -> anyhow::Result<()> {
    let mut f = std::fs::File::create(path)?;
    f.write_all(&[0xFF, 0xFE])?;
    let mut written = 0usize;
    let line = "hello world 💩\n";
    let enc: Vec<u16> = line.encode_utf16().collect();
    let mut buf = Vec::with_capacity(enc.len() * 2);
    for c in enc.iter() { buf.extend_from_slice(&c.to_le_bytes()); }
    let target = mb * 1024 * 1024;
    while written < target {
        f.write_all(&buf)?;
        written += buf.len();
    }
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let path = dir.path().join("bench-utf16.txt");
    let mut mb = 50usize;
    for arg in std::env::args().skip(1) {
        if let Some(v) = arg.strip_prefix("--mb=") { mb = v.parse().unwrap_or(mb); }
    }
    write_utf16_le(&path, mb)?;
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
    println!("utf16: {} bytes={} mb={}", dur, copied, mb);
    for arg in std::env::args().skip(1) {
        if let Some(csv) = arg.strip_prefix("--csv=") {
            let exists = std::path::Path::new(csv).exists();
            let mut f = std::fs::OpenOptions::new().create(true).append(true).open(csv)?;
            if !exists { writeln!(f, "bench,bytes,duration_ms,extra")?; }
            let ms = (std::time::Instant::now() - start).as_millis();
            writeln!(f, "utf16,{},{},mb:{}", copied, ms, mb)?;
        }
    }
    Ok(())
}