use std::pin::Pin;

use anyhow::Result;
use async_compression::tokio::write::ZstdEncoder;
use async_stream::stream;

use log::*;
use tokio::io::{AsyncRead, AsyncWriteExt};
use tokio_stream::StreamExt;
use tokio_util::io::{ReaderStream, StreamReader};

type FinishHandler = dyn FnOnce((u64, Option<Vec<u8>>)) -> Result<()> + Send;
/**
 * wrap a AsyncRead so that it is passthrough,
 * but also the written data is compressed and written into a buffer,
 * unless more than max_cache_size bytes is written, then the cache is dropped and it is pure passthrough.
 */
pub fn async_read_and_write_to_cache<'a>(
    inp: impl AsyncRead + Send + 'a,
    max_cache_size: usize,
    compression_level: i32,
    on_finish: Box<FinishHandler>,
) -> Result<Pin<Box<dyn AsyncRead + Send + 'a>>> {
    let inp = Box::pin(inp);
    let mut zstd_writer = Some(ZstdEncoder::with_quality(
        Vec::new(),
        async_compression::Level::Precise(compression_level as u32),
    ));
    let mut bytes_written = 0;

    let s = stream! {
        let mut stream = ReaderStream::new(inp);
        while let Some(bytes) = stream.next().await {
            if let Ok(bytes) = &bytes {
                if let Some(writer) = zstd_writer.as_mut() {
                    writer.write_all(bytes).await?;
                    bytes_written += bytes.len() as u64;
                    let compressed_len = writer.get_ref().len();
                    trace!("wrote {} to zstd, len now {}", bytes.len(), compressed_len);
                    if compressed_len > max_cache_size {
                        debug!("cache longer than max, dropping");
                        //writer.finish();
                        zstd_writer.take();
                    }
                }
            }
            yield bytes;
        }
        // EOF, call on_finish
        let finish = {
            if let Some(mut writer) = zstd_writer.take() {
                writer.shutdown().await?;
                let res = writer.into_inner();
                if res.len() <= max_cache_size {
                    (bytes_written, Some(res))
                } else {
                    (bytes_written, None)
                }
            } else {
                (bytes_written, None)
            }
        };

        // EOF, finish!
        on_finish(finish)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    };

    Ok(Box::pin(StreamReader::new(s)))
}
