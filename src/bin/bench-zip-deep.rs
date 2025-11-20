use ripgrep_all as rga;
use async_zip::{Compression, ZipEntryBuilder, base::write::ZipFileWriter, ZipString};
use tokio_util::compat::TokioAsyncWriteCompatExt;
use std::io::Write;
use std::time::Instant;
use tempfile::tempdir;
use tokio::fs::File;
use tokio::io::{copy, sink};

async fn create_simple_zip_to_file(dir: &std::path::Path, file_name: &str, entries: usize) -> anyhow::Result<std::path::PathBuf> {
    let out_path = dir.join(file_name);
    let file = tokio::fs::File::create(&out_path).await?;
    let mut zip = ZipFileWriter::new(file.compat_write());
    for i in 0..entries {
        let name = format!("file-{i}.txt");
        let content = format!("hello {i}\n");
        let opts = ZipEntryBuilder::new(ZipString::from(name), Compression::Stored);
        zip.write_entry_whole(opts, content.as_bytes()).await?;
    }
    zip.close().await?;
    Ok(out_path)
}

async fn create_zip(dir: &std::path::Path, entries: usize, nested: bool) -> anyhow::Result<Vec<u8>> {
    let out_path = create_simple_zip_to_file(dir, "outer.zip", entries).await?;
    let mut outer = tokio::fs::read(&out_path).await?;
    if nested {
        let inner_path = create_simple_zip_to_file(dir, "inner.zip", 100).await?;
        let inner = tokio::fs::read(&inner_path).await?;
        let file = tokio::fs::File::create(dir.join("outer_with_inner.zip")).await?;
        let mut zip = ZipFileWriter::new(file.compat_write());
        let opts = ZipEntryBuilder::new(ZipString::from("inner.zip".to_string()), Compression::Stored);
        zip.write_entry_whole(opts, &inner).await?;
        zip.close().await?;
        outer = tokio::fs::read(dir.join("outer_with_inner.zip")).await?;
    }
    Ok(outer)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let path = dir.path().join("bench.zip");
    let mut entries = 1000usize;
    let mut nested = true;
    for arg in std::env::args().skip(1) {
        if let Some(v) = arg.strip_prefix("--entries=") { entries = v.parse().unwrap_or(entries); }
        if let Some(v) = arg.strip_prefix("--nested=") { nested = v.parse::<bool>().unwrap_or(nested); }
    }
    let data = create_zip(dir.path(), entries, nested).await?;
    std::fs::write(&path, data)?;
    let mut cfg = rga::config::RgaConfig { cache: rga::config::CacheConfig { disabled: true, ..Default::default() }, ..Default::default() };
    for arg in std::env::args().skip(1) {
        if let Some(v) = arg.strip_prefix("--rga-zip-max-concurrency=") && let Ok(u) = v.parse::<usize>() { cfg.zip_max_concurrency = rga::config::ZipMaxConcurrency(u); }
        if let Some(v) = arg.strip_prefix("--rga-zip-pipe-bytes=") && let Ok(u) = v.parse::<usize>() { cfg.zip_pipe_bytes = rga::config::ZipPipeBytes(u); }
        if let Some(v) = arg.strip_prefix("--rga-writing-pipe-bytes=") && let Ok(u) = v.parse::<usize>() { cfg.writing_pipe_bytes = rga::config::WritingPipeBytes(u); }
        if let Some(v) = arg.strip_prefix("--rga-postproc-pipe-bytes=") && let Ok(u) = v.parse::<usize>() { cfg.postproc_pipe_bytes = rga::config::PostprocPipeBytes(u); }
        if let Some(v) = arg.strip_prefix("--rga-zip-owned-iter=") && let Ok(u) = v.parse::<bool>() { cfg.zip_owned_iter = u; }
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
    println!("zip-deep: {} bytes={} entries={} nested={}", dur, copied, entries, nested);
    for arg in std::env::args().skip(1) {
        if let Some(csv) = arg.strip_prefix("--csv=") {
            let exists = std::path::Path::new(csv).exists();
            let mut f = std::fs::OpenOptions::new().create(true).append(true).open(csv)?;
            if !exists { writeln!(f, "bench,bytes,duration_ms,extra")?; }
            let ms = (std::time::Instant::now() - start).as_millis();
            writeln!(f, "zip-deep,{},{},entries:{};nested:{}", copied, ms, entries, nested)?;
        }
    }
    Ok(())
}