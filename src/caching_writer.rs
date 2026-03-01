use std::{future::Future, pin::Pin};

use anyhow::{Context, Result};
use async_compression::tokio::write::ZstdEncoder;
use async_stream::stream;

use crate::to_io_err;
use log::*;
use tokio::io::{AsyncRead, AsyncWriteExt};
use tokio_stream::StreamExt;
use tokio_util::io::{ReaderStream, StreamReader};

type FinishHandler =
    dyn FnOnce((u64, Option<Vec<u8>>)) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> + Send;
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
        async_compression::Level::Precise(compression_level.try_into().unwrap()),
    ));
    let mut bytes_written = 0;

    let s = stream! {
        let mut stream = ReaderStream::new(inp);
        while let Some(bytes) = stream.next().await {
            trace!("read bytes: {:?}", bytes);
            if let (Ok(bytes), Some(writer)) = (&bytes, zstd_writer.as_mut()) {
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
            yield bytes;
        }
        trace!("eof");
        // EOF, call on_finish
        let finish = {
            match zstd_writer.take() { Some(mut writer) => {
                writer.shutdown().await?;
                let res = writer.into_inner();
                trace!("EOF");
                if res.len() <= max_cache_size {
                    trace!("writing {} bytes to cache", res.len());
                    (bytes_written, Some(res))
                } else {
                    trace!("cache longer than max, dropping");
                    (bytes_written, None)
                }
            } _ => {
                (bytes_written, None)
            }}
        };

        // EOF, finish!
        on_finish(finish).await.context("write_to_cache on_finish")
            .map_err(to_io_err)?;

    };

    Ok(Box::pin(StreamReader::new(s)))
}
