// https://github.com/arcnmx/pipe-rs/blob/master/src/lib.rs
// extended to support sending io errors

#![deny(missing_docs)]
#![cfg_attr(feature = "unstable-doc-cfg", feature(doc_cfg))]

//! Synchronous in-memory pipe
//!
//! ## Example
//!
//! ```
//! use std::thread::spawn;
//! use std::io::{Read, Write};
//!
//! let (mut read, mut write) = ripgrep_all::pipe::pipe();
//!
//! let message = "Hello, world!";
//! spawn(move || write.write_all(message.as_bytes()).unwrap());
//!
//! let mut s = String::new();
//! read.read_to_string(&mut s).unwrap();
//!
//! assert_eq!(&s, message);
//! ```

use crossbeam_channel::{Receiver, Sender};
use std::cmp::min;
use std::io::{self, BufRead, Read, Result, Write};

/// The `Read` end of a pipe (see `pipe()`)
pub struct PipeReader {
    receiver: Receiver<Result<Vec<u8>>>,
    buffer: Vec<u8>,
    position: usize,
}

/// The `Write` end of a pipe (see `pipe()`)
#[derive(Clone)]
pub struct PipeWriter {
    sender: Sender<Result<Vec<u8>>>,
}

/// Creates a synchronous memory pipe
pub fn pipe() -> (PipeReader, PipeWriter) {
    let (sender, receiver) = crossbeam_channel::bounded(0);

    (
        PipeReader {
            receiver,
            buffer: Vec::new(),
            position: 0,
        },
        PipeWriter { sender },
    )
}

impl PipeWriter {
    /// Extracts the inner `SyncSender` from the writer
    pub fn into_inner(self) -> Sender<Result<Vec<u8>>> {
        self.sender
    }

    /// Write any error into the pipe, will be handled as an IO error
    pub fn write_err(&self, e: std::io::Error) -> Result<()> {
        self.sender
            .send(Err(e))
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "pipe reader has been dropped"))
    }
}

impl PipeReader {
    /// Extracts the inner `Receiver` from the writer, and any pending buffered data
    pub fn into_inner(mut self) -> (Receiver<Result<Vec<u8>>>, Vec<u8>) {
        self.buffer.drain(..self.position);
        (self.receiver, self.buffer)
    }
}

impl BufRead for PipeReader {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        while self.position >= self.buffer.len() {
            match self.receiver.recv() {
                // The only existing error is EOF
                Err(_) => break,
                Ok(Err(e)) => Err(e)?,
                Ok(Ok(data)) => {
                    self.buffer = data;
                    self.position = 0;
                }
            }
        }

        Ok(&self.buffer[self.position..])
    }

    fn consume(&mut self, amt: usize) {
        debug_assert!(self.buffer.len() - self.position >= amt);
        self.position += amt
    }
}

impl Read for PipeReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        let internal = self.fill_buf()?;

        let len = min(buf.len(), internal.len());
        if len > 0 {
            buf[..len].copy_from_slice(&internal[..len]);
            self.consume(len);
        }
        Ok(len)
    }
}

impl Write for PipeWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let data = buf.to_vec();

        self.sender
            .send(Ok(data))
            .map(|_| buf.len())
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "pipe reader has been dropped"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::thread::spawn;

    #[test]
    fn pipe_reader() {
        let i = b"hello there";
        let mut o = Vec::with_capacity(i.len());
        let (mut r, mut w) = pipe();
        let guard = spawn(move || {
            w.write_all(&i[..5]).unwrap();
            w.write_all(&i[5..]).unwrap();
            drop(w);
        });

        r.read_to_end(&mut o).unwrap();
        assert_eq!(i, &o[..]);

        guard.join().unwrap();
    }

    #[test]
    fn pipe_writer_fail() {
        let i = b"hi";
        let (r, mut w) = pipe();
        let guard = spawn(move || {
            drop(r);
        });

        assert!(w.write_all(i).is_err());

        guard.join().unwrap();
    }

    #[test]
    fn small_reads() {
        let block_cnt = 20;
        const BLOCK: usize = 20;
        let (mut r, mut w) = pipe();
        let guard = spawn(move || {
            for _ in 0..block_cnt {
                let data = &[0; BLOCK];
                w.write_all(data).unwrap();
            }
        });

        let mut buff = [0; BLOCK / 2];
        let mut read = 0;
        while let Ok(size) = r.read(&mut buff) {
            // 0 means EOF
            if size == 0 {
                break;
            }
            read += size;
        }
        assert_eq!(block_cnt * BLOCK, read);

        guard.join().unwrap();
    }
}
