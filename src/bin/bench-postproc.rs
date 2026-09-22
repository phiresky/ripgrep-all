use std::time::Instant;

use tokio::io::AsyncReadExt;

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> anyhow::Result<()> {
    let line = "the quick brown fox jumps over the lazy dog and keeps running far away\n";
    let repetitions = 300_000;
    let mut input = Vec::with_capacity(line.len() * repetitions);
    for _ in 0..repetitions {
        input.extend_from_slice(line.as_bytes());
    }
    let prefix = "some/archive.zip:inner/file.txt:";
    for _ in 0..3 {
        let t0 = Instant::now();
        let read =
            ripgrep_all::adapters::postproc::postproc_prefix(prefix, std::io::Cursor::new(&input));
        tokio::pin!(read);
        let mut out = Vec::new();
        read.read_to_end(&mut out).await?;
        println!(
            "postproc_prefix: {} B in, {} B out, {:?} ({:.1} MiB/s)",
            input.len(),
            out.len(),
            t0.elapsed(),
            input.len() as f64 / t0.elapsed().as_secs_f64() / (1 << 20) as f64
        );
    }
    Ok(())
}
