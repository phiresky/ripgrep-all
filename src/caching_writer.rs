use std::{pin::Pin, task::Poll};

use anyhow::Result;
use async_compression::tokio::write::ZstdEncoder;
use log::*;
use tokio::{
    io::{AsyncRead, AsyncWrite, AsyncWriteExt},
    pin,
};

/**
 * wrap a writer so that it is passthrough,
 * but also the written data is compressed and written into a buffer,
 * unless more than max_cache_size bytes is written, then the cache is dropped and it is pure passthrough.
 */
pub struct CachingReader<R: AsyncRead> {
    max_cache_size: usize,
    // set to none if the size goes over the limit
    zstd_writer: Option<ZstdEncoder<Vec<u8>>>,
    inp: Pin<Box<R>>,
    bytes_written: u64,
    on_finish: Box<dyn FnOnce((u64, Option<Vec<u8>>)) -> Result<()> + Send>,
}
impl<R: AsyncRead> CachingReader<R> {
    pub fn new(
        inp: R,
        max_cache_size: usize,
        compression_level: i32,
        on_finish: Box<dyn FnOnce((u64, Option<Vec<u8>>)) -> Result<()> + Send>,
    ) -> Result<CachingReader<R>> {
        Ok(CachingReader {
            inp: Box::pin(inp),
            max_cache_size,
            zstd_writer: Some(ZstdEncoder::with_quality(
                Vec::new(),
                async_compression::Level::Precise(compression_level as u32),
            )),
            bytes_written: 0,
            on_finish,
        })
    }
    pub fn finish(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::io::Result<(u64, Option<Vec<u8>>)> {
        if let Some(writer) = self.zstd_writer.take() {
            pin!(writer);
            writer.as_mut().poll_shutdown(cx)?;
            let res = writer.get_pin_mut().clone(); // TODO: without copying possible?
            if res.len() <= self.max_cache_size {
                return Ok((self.bytes_written, Some(res)));
            }
        }
        Ok((self.bytes_written, None))
    }
    async fn write_to_compressed(&mut self, buf: &[u8]) -> std::io::Result<()> {
        if let Some(writer) = self.zstd_writer.as_mut() {
            writer.write_all(buf).await?;
            let compressed_len = writer.get_ref().len();
            trace!("wrote {} to zstd, len now {}", buf.len(), compressed_len);
            if compressed_len > self.max_cache_size {
                debug!("cache longer than max, dropping");
                //writer.finish();
                self.zstd_writer.take();
            }
        }
        Ok(())
    }
}
impl<R> AsyncRead for CachingReader<R>
where
    R: AsyncRead,
{
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        mut buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let old_filled_len = buf.filled().len();
        match self.inp.as_mut().poll_read(cx, &mut buf) {
            /*Ok(0) => {

            }
            Ok(read_bytes) => {
                self.write_to_compressed(&buf[0..read_bytes])?;
                self.bytes_written += read_bytes as u64;
                Ok(read_bytes)
            }*/
            Poll::Ready(rdy) => {
                if let Ok(()) = &rdy {
                    let slice = buf.filled();
                    let read_bytes = slice.len() - old_filled_len;
                    if read_bytes == 0 {
                        // EOF
                        // move out of box, replace with noop lambda
                        let on_finish =
                            std::mem::replace(&mut self.on_finish, Box::new(|_| Ok(())));
                        // EOF, finish!
                        (on_finish)(self.finish(cx)?)
                            .map(|()| 0)
                            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
                    } else {
                        self.write_to_compressed(&slice[old_filled_len..]);
                        self.bytes_written += read_bytes as u64;
                    }
                }
                Poll::Ready(rdy)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}
