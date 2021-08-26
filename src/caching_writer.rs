use anyhow::Result;
use log::*;
use std::io::{Read, Write};

/**
 * wrap a writer so that it is passthrough,
 * but also the written data is compressed and written into a buffer,
 * unless more than max_cache_size bytes is written, then the cache is dropped and it is pure passthrough.
 */
pub struct CachingReader<R: Read> {
    max_cache_size: usize,
    zstd_writer: Option<zstd::stream::write::Encoder<'static, Vec<u8>>>,
    inp: R,
    bytes_written: u64,
    on_finish: Box<dyn FnOnce((u64, Option<Vec<u8>>)) -> Result<()> + Send>,
}
impl<R: Read> CachingReader<R> {
    pub fn new(
        inp: R,
        max_cache_size: usize,
        compression_level: i32,
        on_finish: Box<dyn FnOnce((u64, Option<Vec<u8>>)) -> Result<()> + Send>,
    ) -> Result<CachingReader<R>> {
        Ok(CachingReader {
            inp,
            max_cache_size,
            zstd_writer: Some(zstd::stream::write::Encoder::new(
                Vec::new(),
                compression_level,
            )?),
            bytes_written: 0,
            on_finish,
        })
    }
    pub fn finish(&mut self) -> std::io::Result<(u64, Option<Vec<u8>>)> {
        if let Some(writer) = self.zstd_writer.take() {
            let res = writer.finish()?;
            if res.len() <= self.max_cache_size {
                return Ok((self.bytes_written, Some(res)));
            }
        }
        Ok((self.bytes_written, None))
    }
    fn write_to_compressed(&mut self, buf: &[u8]) -> std::io::Result<()> {
        if let Some(writer) = self.zstd_writer.as_mut() {
            let wrote = writer.write(buf)?;
            let compressed_len = writer.get_ref().len();
            trace!("wrote {} to zstd, len now {}", wrote, compressed_len);
            if compressed_len > self.max_cache_size {
                debug!("cache longer than max, dropping");
                //writer.finish();
                self.zstd_writer.take().unwrap().finish()?;
            }
        }
        Ok(())
    }
}
impl<R: Read> Read for CachingReader<R> {
    fn read(&mut self, mut buf: &mut [u8]) -> std::io::Result<usize> {
        match self.inp.read(&mut buf) {
            Ok(0) => {
                // move out of box, replace with noop lambda
                let on_finish = std::mem::replace(&mut self.on_finish, Box::new(|_| Ok(())));
                // EOF, finish!
                (on_finish)(self.finish()?)
                    .map(|()| 0)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
            }
            Ok(read_bytes) => {
                self.write_to_compressed(&buf[0..read_bytes])?;
                self.bytes_written += read_bytes as u64;
                Ok(read_bytes)
            }
            Err(e) => Err(e),
        }
    }
}
