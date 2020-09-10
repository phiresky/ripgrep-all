//trait RunFnAdapter: GetMetadata {}

//impl<T> FileAdapter for T where T: RunFnAdapter {}

use anyhow::Result;
use std::io::{BufRead, BufReader};
use std::{
    cmp::min,
    io::{Read, Write},
};

struct ByteReplacer<R>
where
    R: Read,
{
    inner: R,
    next_read: Vec<u8>,
    replacer: Box<dyn FnMut(u8) -> Vec<u8>>,
    haystacker: Box<dyn Fn(&[u8]) -> Option<usize>>,
}

impl<R> ByteReplacer<R>
where
    R: Read,
{
    fn output_next(&mut self, buf: &mut [u8], buf_valid_until: usize, replacement: &[u8]) -> usize {
        let after_part1 = Vec::from(&buf[1..buf_valid_until]);

        /*let mut after_part = Vec::with_capacity(replacement.len() + replaced_len);
        after_part.extend_from_slice(replacement);
        after_part.extend_from_slice(&buf[..replaced_len]);*/

        let writeable_count = min(buf.len(), replacement.len());
        buf[..writeable_count].copy_from_slice(&replacement[0..writeable_count]);

        let after_rep = &replacement[writeable_count..];
        let mut ov = Vec::new();
        ov.extend_from_slice(&after_rep);
        ov.extend_from_slice(&after_part1);
        ov.extend_from_slice(&self.next_read);
        self.next_read = ov;

        return writeable_count;
    }
}

impl<R> Read for ByteReplacer<R>
where
    R: Read,
{
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let read = if self.next_read.len() > 0 {
            let count = std::cmp::min(self.next_read.len(), buf.len());
            buf[0..count].copy_from_slice(&self.next_read[0..count]);
            self.next_read.drain(0..count).count();
            Ok(count)
        } else {
            self.inner.read(buf)
        };

        match read {
            Ok(u) => {
                match (self.haystacker)(&buf[0..u]) {
                    Some(i) => {
                        let data = (self.replacer)(buf[i]);

                        Ok(i + self.output_next(&mut buf[i..], u - i, &data))
                    }
                    None => Ok(u),
                }
                // todo: use memchr2?
            }
            Err(e) => Err(e),
        }
    }
}

pub fn postprocB(_line_prefix: &str, inp: impl Read) -> Result<impl Read> {
    let mut page_count = 1;

    Ok(ByteReplacer {
        inner: inp,
        next_read: Vec::new(),
        haystacker: Box::new(|buf| memchr::memchr2(b'\n', b'\x0c', buf)),
        replacer: Box::new(move |b| match b {
            b'\n' => format!("\nPage {}:", page_count).into_bytes(),
            b'\x0c' => {
                page_count += 1;
                format!("\nPage {}:", page_count).into_bytes()
            }
            _ => b"[[imposs]]".to_vec(),
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::postprocB;
    use anyhow::Result;
    use std::io::Read;

    fn test_from_strs(a: &str, b: &str) -> Result<()> {
        let mut oup = Vec::new();
        postprocB("", a.as_bytes())?.read_to_end(&mut oup)?;
        let c = String::from_utf8_lossy(&oup);
        if b != c {
            anyhow::bail!("{}\nshould be\n{}\nbut is\n{}", a, b, c);
        }

        Ok(())
    }

    #[test]
    fn post1() -> Result<()> {
        let inp = "What is this\nThis is a test\nFoo";
        let oup = "What is this\nPage 1:This is a test\nPage 1:Foo";

        test_from_strs(inp, oup)?;

        println!("\n\n\n\n");

        let inp = "What is this\nThis is a test\nFoo\x0c\nHelloooo\nHow are you?\x0c\nGreat!";
        let oup = "What is this\nPage 1:This is a test\nPage 1:Foo\nPage 2:\nPage 2:Helloooo\nPage 2:How are you?\nPage 3:\nPage 3:Great!";

        test_from_strs(inp, oup)?;

        Ok(())
    }
}
