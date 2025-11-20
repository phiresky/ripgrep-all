use std::{future::Future, pin::Pin};

use anyhow::{Context, Result};
use async_compression::tokio::write::ZstdEncoder;
use async_stream::stream;

use crate::to_io_err;
use lazy_static::lazy_static;
use log::*;
use tokio::io::{AsyncRead, AsyncWriteExt};
use tokio_stream::StreamExt;
use tokio_util::io::{ReaderStream, StreamReader};
use std::sync::Mutex;
use bytes::BytesMut;
use tokio::io::AsyncWrite;
use std::task::{Context as TaskContext, Poll};

type FinishHandler =
    dyn FnOnce((u64, Option<Vec<u8>>)) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> + Send;

lazy_static! {
    #[cfg(feature = "cache-dict-train")]
    static ref ZSTD_DICT_BYTES: Mutex<Option<Vec<u8>>> = Mutex::new(None);
    #[cfg(feature = "cache-dict-train")]
    static ref ZSTD_TRAIN_SAMPLES: Mutex<Vec<Vec<u8>>> = Mutex::new(Vec::new());
}

fn maybe_train_zstd_dict(sample: Vec<u8>) {
    #[cfg(feature = "cache-dict-train")]
    {
        let mut dict_guard = ZSTD_DICT_BYTES.lock().unwrap();
        if dict_guard.is_some() { return; }
        let mut samples = ZSTD_TRAIN_SAMPLES.lock().unwrap();
        samples.push(sample);
        const MIN_SAMPLES: usize = 16;
        const MAX_DICT_SIZE: usize = 1 << 14; // 16 KiB
        if samples.len() >= MIN_SAMPLES {
            let refs: Vec<&[u8]> = samples.iter().map(|v| v.as_slice()).collect();
            match zstd::dict::from_samples(&refs, MAX_DICT_SIZE) {
                Ok(dict) => { *dict_guard = Some(dict); }
                Err(e) => { debug!("training zstd dict failed: {:?}", e); }
            }
            samples.clear();
        }
    }
}

pub fn load_zstd_dict_path(path: &str) -> Result<()> {
    #[cfg(feature = "cache-dict-train")]
    {
        let mut dict_guard = ZSTD_DICT_BYTES.lock().unwrap();
        if dict_guard.is_some() { return Ok(()); }
        let bytes = std::fs::read(path).with_context(|| format!("reading zstd dict {path}"))?;
        *dict_guard = Some(bytes);
    }
    Ok(())
}
/**
 * wrap a AsyncRead so that it is passthrough,
 * but also the written data is compressed and written into a buffer,
 * unless more than max_cache_size bytes is written, then the cache is dropped and it is pure passthrough.
 */
pub fn async_read_and_write_to_cache<'a>(
    inp: impl AsyncRead + Send + 'a,
    max_cache_size: usize,
    compression_level: i32,
    small_uncompressed_bytes: usize,
    on_finish: Box<FinishHandler>,
) -> Result<Pin<Box<dyn AsyncRead + Send + 'a>>> {
    let inp = Box::pin(inp);
    let mut zstd_writer: Option<ZstdEncoder<BytesMutWriter>> = None;
    let small_fast_path: usize = 1 << 14; // 16 KiB
    let medium_threshold: u64 = 1 << 20; // 1 MiB
    let mut small_buf: Option<Vec<u8>> = Some(Vec::with_capacity(small_fast_path));
    let mut bytes_written = 0;

    let s = stream! {
        let mut stream = ReaderStream::new(inp);
        while let Some(bytes) = stream.next().await {
            trace!("read bytes: {:?}", bytes);
            if let Ok(bytes) = &bytes {
                bytes_written += bytes.len() as u64;
                if let Some(buf) = small_buf.as_mut() {
                    if buf.len() + bytes.len() <= small_fast_path {
                        buf.extend_from_slice(bytes);
                    } else {
                        let lvl = if bytes_written <= medium_threshold { 1 } else { compression_level };
                        let mut writer = ZstdEncoder::with_quality(BytesMutWriter::new(1<<15), async_compression::Level::Precise(lvl));
                        writer.write_all(buf).await?;
                        writer.write_all(bytes).await?;
                        zstd_writer = Some(writer);
                        small_buf.take();
                    }
                } else if let Some(writer) = zstd_writer.as_mut() {
                    writer.write_all(bytes).await?;
                    let compressed_len = writer.get_ref().inner.len();
                    trace!("wrote {} to zstd, len now {}", bytes.len(), compressed_len);
                    if compressed_len > max_cache_size {
                        debug!("cache longer than max, dropping");
                        zstd_writer.take();
                    }
                }
            }
            yield bytes;
        }
        trace!("eof");
        // EOF, call on_finish
        let finish = {
            if let Some(mut writer) = zstd_writer.take() {
                writer.shutdown().await?;
                let res = writer.into_inner().into_inner().to_vec();
                trace!("EOF");
                let ratio_bad = if bytes_written > 0 { (res.len() as f64) / (bytes_written as f64) > 0.9 } else { false };
                if res.len() <= max_cache_size && !ratio_bad {
                    trace!("writing {} bytes to cache", res.len());
                    (bytes_written, Some(res))
                } else {
                    trace!("cache longer than max, dropping");
                    (bytes_written, None)
                }
            } else if let Some(buf) = small_buf.take() {
                if bytes_written as usize <= small_uncompressed_bytes {
                    (bytes_written, Some(buf))
                } else {
                // compress small outputs once with level 1
                // try dictionary if available, otherwise zstd level 1
                let dict_opt = {
                    #[cfg(feature = "cache-dict-train")]
                    { ZSTD_DICT_BYTES.lock().unwrap().clone() }
                    #[cfg(not(feature = "cache-dict-train"))]
                    { None }
                };
                if let Some(dict_bytes) = dict_opt {
                    match (|| {
                        let mut enc = zstd::Encoder::with_dictionary(Vec::new(), 1, &dict_bytes)?;
                        std::io::Write::write_all(&mut enc, &buf)?;
                        let res = enc.finish()?;
                        Ok::<Vec<u8>, std::io::Error>(res)
                    })() {
                        Ok(res) => {
                            let ratio_bad = if bytes_written > 0 { (res.len() as f64) / (bytes_written as f64) > 0.9 } else { false };
                            if res.len() <= max_cache_size && !ratio_bad { (bytes_written, Some(res)) } else { (bytes_written, None) }
                        }
                        Err(_) => {
                            let mut writer = ZstdEncoder::with_quality(Vec::new(), async_compression::Level::Precise(1));
                            writer.write_all(&buf).await?;
                            writer.shutdown().await?;
                            let res = writer.into_inner();
                            let ratio_bad = if bytes_written > 0 { (res.len() as f64) / (bytes_written as f64) > 0.9 } else { false };
                            if res.len() <= max_cache_size && !ratio_bad { (bytes_written, Some(res)) } else { (bytes_written, None) }
                        }
                    }
                } else {
                    maybe_train_zstd_dict(buf.clone());
                    let mut writer = ZstdEncoder::with_quality(Vec::new(), async_compression::Level::Precise(1));
                    writer.write_all(&buf).await?;
                    writer.shutdown().await?;
                    let res = writer.into_inner();
                    let ratio_bad = if bytes_written > 0 { (res.len() as f64) / (bytes_written as f64) > 0.9 } else { false };
                    if res.len() <= max_cache_size && !ratio_bad { (bytes_written, Some(res)) } else { (bytes_written, None) }
                }
                }
            } else {
                (bytes_written, None)
            }
        };

        // EOF, finish!
        on_finish(finish).await.context("write_to_cache on_finish")
            .map_err(to_io_err)?;

    };

    Ok(Box::pin(StreamReader::new(s)))
}
struct BytesMutWriter {
    inner: BytesMut,
}
impl BytesMutWriter {
    fn new(cap: usize) -> Self { Self { inner: BytesMut::with_capacity(cap) } }
    fn into_inner(self) -> BytesMut { self.inner }
}
impl AsyncWrite for BytesMutWriter {
    fn poll_write(self: Pin<&mut Self>, _cx: &mut TaskContext<'_>, buf: &[u8]) -> Poll<std::io::Result<usize>> {
        let me = unsafe { self.get_unchecked_mut() };
        me.inner.extend_from_slice(buf);
        Poll::Ready(Ok(buf.len()))
    }
    fn poll_flush(self: Pin<&mut Self>, _cx: &mut TaskContext<'_>) -> Poll<std::io::Result<()>> { Poll::Ready(Ok(())) }
    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut TaskContext<'_>) -> Poll<std::io::Result<()>> { Poll::Ready(Ok(())) }
}
