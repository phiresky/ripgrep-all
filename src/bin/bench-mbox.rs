use ripgrep_all as rga;
use std::io::Write;
use std::time::Instant;
use tempfile::tempdir;
use tokio::fs::File;
use tokio::io::{copy, sink};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let path = dir.path().join("bench.mbox");
    {
        let mut f = std::fs::File::create(&path)?;
        let mut msg = String::new();
        msg.push_str("Content-Type: text/plain; charset=UTF-8\n\n");
        msg.push_str("Hello world\n");
        let mut n = 10000;
        for arg in std::env::args().skip(1) {
            if let Some(v) = arg.strip_prefix("--messages=") { n = v.parse().unwrap_or(n); }
        }
        for i in 0..n {
            write!(f, "From user@example.com\n")?;
            write!(f, "Message {i}\n")?;
            f.write_all(msg.as_bytes())?;
        }
    }
    let mut cfg = rga::config::RgaConfig::default();
    cfg.accurate = true;
    cfg.cache.disabled = true;
    cfg.adapters = vec!["+mail".to_string()];
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
    println!("mbox: {} bytes={} messages={}", dur, copied, n);
    for arg in std::env::args().skip(1) {
        if let Some(csv) = arg.strip_prefix("--csv=") {
            let exists = std::path::Path::new(csv).exists();
            let mut f = std::fs::OpenOptions::new().create(true).append(true).open(csv)?;
            if !exists { writeln!(f, "bench,bytes,duration_ms,extra")?; }
            let ms = (std::time::Instant::now() - start).as_millis();
            writeln!(f, "mbox,{},{},messages:{}", copied, ms, n)?;
        }
    }
    Ok(())
}