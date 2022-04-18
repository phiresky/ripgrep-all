//trait RunFnAdapter: GetMetadata {}

//impl<T> FileAdapter for T where T: RunFnAdapter {}

use anyhow::Context;
use anyhow::Result;
use encoding_rs_io::DecodeReaderBytesBuilder;

use std::{
    cmp::min,
    io::{BufRead, BufReader, Read},
};

use crate::adapted_iter::{AdaptedFilesIterBox, SingleAdaptedFileAsIter};

use super::{AdaptInfo, AdapterMeta, FileAdapter, GetMetadata};

/** pass through, except adding \n at the end */
pub struct EnsureEndsWithNewline<R: Read> {
    inner: R,
    added_newline: bool,
}
impl<R: Read> EnsureEndsWithNewline<R> {
    pub fn new(r: R) -> EnsureEndsWithNewline<R> {
        EnsureEndsWithNewline {
            inner: r,
            added_newline: false,
        }
    }
}
impl<R: Read> Read for EnsureEndsWithNewline<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self.inner.read(buf) {
            Ok(0) => {
                if self.added_newline {
                    Ok(0)
                } else {
                    buf[0] = b'\n';
                    self.added_newline = true;
                    Ok(1)
                }
            }
            Ok(n) => Ok(n),
            Err(e) => Err(e),
        }
    }
}
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
                description: "Adds the line prefix to each line (e.g. the filename within a zip)".to_owned(),
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
    ) -> Result<AdaptedFilesIterBox<'a>> {
        let read = EnsureEndsWithNewline::new(postproc_prefix(
            &a.line_prefix,
            postproc_encoding(&a.line_prefix, a.inp)?,
        ));
        // keep adapt info (filename etc) except replace inp
        let ai = AdaptInfo {
            inp: Box::new(read),
            postprocess: false,
            ..a
        };
        Ok(Box::new(SingleAdaptedFileAsIter::new(ai)))
    }
}

/*struct ReadErr {
    err: Fn() -> std::io::Error,
}
impl Read for ReadErr {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        Err(self.err())
    }
}*/

pub fn postproc_encoding<'a, R: Read + 'a>(
    line_prefix: &str,
    inp: R,
) -> Result<Box<dyn Read + 'a>> {
    // TODO: parse these options from ripgrep's configuration
    let encoding = None; // detect bom but usually assume utf8
    let bom_sniffing = true;
    let mut decode_builder = DecodeReaderBytesBuilder::new();
    // https://github.com/BurntSushi/ripgrep/blob/a7d26c8f144a4957b75f71087a66692d0b25759a/grep-searcher/src/searcher/mod.rs#L706
    // this detects utf-16 BOMs and transcodes to utf-8 if they are present
    // it does not detect any other char encodings. that would require https://github.com/hsivonen/chardetng or similar but then binary detection is hard (?)
    let inp = decode_builder
        .encoding(encoding)
        .utf8_passthru(true)
        .strip_bom(bom_sniffing)
        .bom_override(true)
        .bom_sniffing(bom_sniffing)
        .build(inp);

    // check for binary content in first 8kB
    // read the first 8kB into a buffer, check for null bytes, then return the buffer concatenated with the rest of the file
    let mut fourk = Vec::with_capacity(1 << 13);
    let mut beginning = inp.take(1 << 13);

    beginning.read_to_end(&mut fourk)?;

    if fourk.contains(&0u8) {
        log::debug!("detected binary");
        let v = "[rga: binary data]";
        return Ok(Box::new(std::io::Cursor::new(v)));
        /*let err = std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("{}[rga: binary data]", line_prefix),
        );
        return Err(err).context("");
        return ReadErr {
            err,
        };*/
    }
    Ok(Box::new(
        std::io::Cursor::new(fourk).chain(beginning.into_inner()),
    ))
}

pub fn postproc_prefix(line_prefix: &str, inp: impl Read) -> impl Read {
    let line_prefix = line_prefix.to_string(); // clone since we need it later
    ByteReplacer {
        inner: inp,
        next_read: format!("{}", line_prefix).into_bytes(),
        haystacker: Box::new(|buf| memchr::memchr(b'\n', buf)),
        replacer: Box::new(move |_| format!("\n{}", line_prefix).into_bytes()),
    }
}

pub fn postproc_pagebreaks(line_prefix: &str, inp: impl Read) -> impl Read {
    let line_prefix = line_prefix.to_string(); // clone since
    let mut page_count = 1;

    ByteReplacer {
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use std::io::Read;

    fn test_from_strs(pagebreaks: bool, line_prefix: &str, a: &str, b: &str) -> Result<()> {
        test_from_bytes(pagebreaks, line_prefix, a.as_bytes(), b)
    }

    fn test_from_bytes(pagebreaks: bool, line_prefix: &str, a: &[u8], b: &str) -> Result<()> {
        let mut oup = Vec::new();
        let inp = postproc_encoding("", a)?;
        if pagebreaks {
            postproc_pagebreaks(line_prefix, inp).read_to_end(&mut oup)?;
        } else {
            postproc_prefix(line_prefix, inp).read_to_end(&mut oup)?;
        }
        let c = String::from_utf8_lossy(&oup);
        if b != c {
            anyhow::bail!(
                "`{}`\nshould be\n`{}`\nbut is\n`{}`",
                String::from_utf8_lossy(&a),
                b,
                c
            );
        }

        Ok(())
    }

    #[test]
    fn post1() -> Result<()> {
        let inp = "What is this\nThis is a test\nFoo";
        let oup = "Page 1:What is this\nPage 1:This is a test\nPage 1:Foo";

        test_from_strs(true, "", inp, oup)?;

        println!("\n\n\n\n");

        let inp = "What is this\nThis is a test\nFoo\x0c\nHelloooo\nHow are you?\x0c\nGreat!";
        let oup = "Page 1:What is this\nPage 1:This is a test\nPage 1:Foo\nPage 2:\nPage 2:Helloooo\nPage 2:How are you?\nPage 3:\nPage 3:Great!";

        test_from_strs(true, "", inp, oup)?;

        let inp = "What is this\nThis is a test\nFoo\x0c\nHelloooo\nHow are you?\x0c\nGreat!";
        let oup = "foo.pdf:What is this\nfoo.pdf:This is a test\nfoo.pdf:Foo\x0c\nfoo.pdf:Helloooo\nfoo.pdf:How are you?\x0c\nfoo.pdf:Great!";

        test_from_strs(false, "foo.pdf:", inp, oup)?;

        test_from_strs(
            false,
            "foo:",
            "this is a test \n\n \0 foo",
            "foo:[rga: binary data]",
        )?;
        test_from_strs(false, "foo:", "\0", "foo:[rga: binary data]")?;

        Ok(())
    }

    /*#[test]
    fn chardet() -> Result<()> {
        let mut d = chardetng::EncodingDetector::new();
        let mut v = Vec::new();
        std::fs::File::open("/home/phire/passwords-2018.kdbx.old").unwrap().read_to_end(&mut v).unwrap();
        d.feed(&v, false);
        println!("foo {:?}", d.guess(None, true));
        Ok(())
    }*/
}
