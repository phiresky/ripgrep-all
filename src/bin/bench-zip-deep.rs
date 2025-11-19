use ripgrep_all as rga;
use async_zip::{Compression, ZipEntryBuilder, write::ZipFileWriter};
use std::time::Instant;
use tempfile::tempdir;
use tokio::fs::File;
use tokio::io::{copy, sink};

fn create_zip(entries: usize, nested: bool) -> anyhow::Result<Vec<u8>> {
    let v = Vec::new();
    let mut cursor = std::io::Cursor::new(v);
    let mut zip = ZipFileWriter::new(&mut cursor);
    for i in 0..entries {
        let name = format!("file-{i}.txt");
        let content = format!("hello {i}\n");
        let opts = ZipEntryBuilder::new(name, Compression::Stored);
        futures::executor::block_on(zip.write_entry_whole(opts, content.as_bytes()))?;
    }
    if nested {
        let inner = create_zip(100, false)?;
        let opts = ZipEntryBuilder::new("inner.zip".to_string(), Compression::Stored);
        futures::executor::block_on(zip.write_entry_whole(opts, &inner))?;
    }
    futures::executor::block_on(zip.close())?;
    Ok(cursor.into_inner())
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
    let data = create_zip(entries, nested)?;
    std::fs::write(&path, data)?;
    let mut cfg = rga::config::RgaConfig::default();
    cfg.cache.disabled = true;
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