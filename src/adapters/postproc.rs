//trait RunFnAdapter: GetMetadata {}

//impl<T> FileAdapter for T where T: RunFnAdapter {}

use anyhow::Result;

use std::{cmp::min, io::Read};

use super::{AdaptInfo, AdapterMeta, FileAdapter, GetMetadata, SingleReadIter};

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

pub struct PostprocPrefix {}
impl GetMetadata for PostprocPrefix {
    fn metadata(&self) -> &super::AdapterMeta {
        lazy_static::lazy_static! {
            static ref METADATA: AdapterMeta = AdapterMeta {
                name: "postprocprefix".to_owned(),
                version: 1,
                description: "Adds the line prefix to each line".to_owned(),
                recurses: true,
                fast_matchers: vec![],
                slow_matchers: None,
                keep_fast_matchers_if_accurate: false,
                disabled_by_default: false
            };
        }
        &METADATA
    }
}
impl FileAdapter for PostprocPrefix {
    fn adapt<'a>(
        &self,
        a: super::AdaptInfo<'a>,
        _detection_reason: &crate::matching::FileMatcher,
    ) -> Result<Box<dyn super::ReadIter + 'a>> {
        let read = postproc_prefix(&a.line_prefix, a.inp)?;
        // keep adapt info (filename etc) except replace inp
        let ai = AdaptInfo {
            inp: Box::new(read),
            postprocess: false,
            ..a
        };
        Ok(Box::new(SingleReadIter::new(ai)))
    }
}

pub fn postproc_prefix(line_prefix: &str, inp: impl Read) -> Result<impl Read> {
    let line_prefix = line_prefix.to_string(); // clone since we need it later
    Ok(ByteReplacer {
        inner: inp,
        next_read: format!("{}", line_prefix).into_bytes(),
        haystacker: Box::new(|buf| memchr::memchr(b'\n', buf)),
        replacer: Box::new(move |_| format!("\n{}", line_prefix).into_bytes()),
    })
}

pub fn postproc_pagebreaks(line_prefix: &str, inp: impl Read) -> Result<impl Read> {
    let line_prefix = line_prefix.to_string(); // clone since
    let mut page_count = 1;

    Ok(ByteReplacer {
        inner: inp,
        next_read: format!("{}Page {}:", line_prefix, page_count).into_bytes(),
        haystacker: Box::new(|buf| memchr::memchr2(b'\n', b'\x0c', buf)),
        replacer: Box::new(move |b| match b {
            b'\n' => format!("\n{}Page {}:", line_prefix, page_count).into_bytes(),
            b'\x0c' => {
                page_count += 1;
                format!("\n{}Page {}:", line_prefix, page_count).into_bytes()
            }
            _ => b"[[imposs]]".to_vec(),
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::postproc_pagebreaks;
    use anyhow::Result;
    use std::io::Read;

    fn test_from_strs(a: &str, b: &str) -> Result<()> {
        let mut oup = Vec::new();
        postproc_pagebreaks("", a.as_bytes())?.read_to_end(&mut oup)?;
        let c = String::from_utf8_lossy(&oup);
        if b != c {
            anyhow::bail!("{}\nshould be\n{}\nbut is\n{}", a, b, c);
        }

        Ok(())
    }

    #[test]
    fn post1() -> Result<()> {
        let inp = "What is this\nThis is a test\nFoo";
        let oup = "Page 1:What is this\nPage 1:This is a test\nPage 1:Foo";

        test_from_strs(inp, oup)?;

        println!("\n\n\n\n");

        let inp = "What is this\nThis is a test\nFoo\x0c\nHelloooo\nHow are you?\x0c\nGreat!";
        let oup = "Page 1:What is this\nPage 1:This is a test\nPage 1:Foo\nPage 2:\nPage 2:Helloooo\nPage 2:How are you?\nPage 3:\nPage 3:Great!";

        test_from_strs(inp, oup)?;

        Ok(())
    }
}
