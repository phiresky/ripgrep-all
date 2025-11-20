use ripgrep_all as rga;
use std::fs::File as StdFile;
use std::io::Write;
use std::time::Instant;
use tar::Builder;
use tempfile::tempdir;
use tokio::fs::File;
use tokio::io::{copy, sink};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let path = dir.path().join("bench.tar");
    {
        let f = StdFile::create(&path)?;
        let mut b = Builder::new(f);
        let mut entries = 1000usize;
        for arg in std::env::args().skip(1) {
            if let Some(v) = arg.strip_prefix("--entries=") { entries = v.parse().unwrap_or(entries); }
        }
        for i in 0..entries {
            let name = format!("dir/file-{i}.txt");
            let mut data = Vec::new();
            for j in 0..100 {
                writeln!(data, "line {i}-{j}")?;
            }
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_mtime(0);
            header.set_uid(0);
            header.set_gid(0);
            header.set_entry_type(tar::EntryType::Regular);
            b.append_data(&mut header, name, std::io::Cursor::new(data))?;
        }
        b.finish()?;
    }
    let mut cfg = rga::config::RgaConfig { cache: rga::config::CacheConfig { disabled: true, ..Default::default() }, ..Default::default() };
    for arg in std::env::args().skip(1) {
        if let Some(v) = arg.strip_prefix("--rga-decompress-gzip-buf-bytes=") && let Ok(u) = v.parse::<usize>() { cfg.decompress_gzip_buf_bytes = rga::config::DecompressGzipBufBytes(u); }
        if let Some(v) = arg.strip_prefix("--rga-decompress-bzip2-buf-bytes=") && let Ok(u) = v.parse::<usize>() { cfg.decompress_bzip2_buf_bytes = rga::config::DecompressBzip2BufBytes(u); }
        if let Some(v) = arg.strip_prefix("--rga-decompress-xz-buf-bytes=") && let Ok(u) = v.parse::<usize>() { cfg.decompress_xz_buf_bytes = rga::config::DecompressXzBufBytes(u); }
        if let Some(v) = arg.strip_prefix("--rga-decompress-zstd-buf-bytes=") && let Ok(u) = v.parse::<usize>() { cfg.decompress_zstd_buf_bytes = rga::config::DecompressZstdBufBytes(u); }
    }
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
    println!("tar: {} bytes={}", dur, copied);
    for arg in std::env::args().skip(1) {
        if let Some(csv) = arg.strip_prefix("--csv=") {
            let exists = std::path::Path::new(csv).exists();
            let mut f = std::fs::OpenOptions::new().create(true).append(true).open(csv)?;
            if !exists { writeln!(f, "bench,bytes,duration_ms,extra")?; }
            let ms = (std::time::Instant::now() - start).as_millis();
            writeln!(f, "tar,{},{},entries:unknown", copied, ms)?;
        }
    }
    Ok(())
}
