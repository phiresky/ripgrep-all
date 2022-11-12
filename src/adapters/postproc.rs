//trait RunFnAdapter: GetMetadata {}

//impl<T> FileAdapter for T where T: RunFnAdapter {}

use anyhow::Context;
use anyhow::Result;
use bytes::Bytes;
use encoding_rs_io::DecodeReaderBytesBuilder;
use tokio::io::{AsyncRead, AsyncReadExt};
use async_stream::stream;
use tokio_util::io::ReaderStream;
use tokio_util::io::StreamReader;
use std::io::Cursor;
use std::pin::Pin;
use std::{
    cmp::min,
};

use crate::adapted_iter::{AdaptedFilesIterBox};

use super::{AdaptInfo, AdapterMeta, FileAdapter, GetMetadata};

fn add_newline(ar: impl AsyncRead + Send) -> impl AsyncRead + Send {
    ar.chain(Cursor::new(&[b'\n']))
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
        a: super::AdaptInfo,
        _detection_reason: &crate::matching::FileMatcher,
    ) -> Result<AdaptedFilesIterBox> {
        let read = add_newline(postproc_prefix(
            &a.line_prefix,
            postproc_encoding(&a.line_prefix, a.inp)?,
        ));
        // keep adapt info (filename etc) except replace inp
        let ai = AdaptInfo {
            inp: Box::pin(read),
            postprocess: false,
            ..a
        };
        Ok(Box::pin(tokio_stream::once(ai)))
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

pub fn postproc_encoding(
    line_prefix: &str,
    inp: impl AsyncRead + Send + 'static,
) -> Result<Pin<Box<dyn AsyncRead + Send>>> {
    Ok(Box::pin(inp))
    // panic!("todo: implement");
    /*// TODO: parse these options from ripgrep's configuration
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
    ))*/
}

pub fn postproc_prefix(line_prefix: &str, inp: impl AsyncRead + Send) -> impl AsyncRead + Send {
    let line_prefix_n = format!("\n{}", line_prefix); // clone since we need it later
    let line_prefix_o = Bytes::copy_from_slice(line_prefix.as_bytes());
    let regex = regex::bytes::Regex::new("\n").unwrap();
    let mut inp_stream = ReaderStream::new(inp);
    let oup_stream = stream! {
        yield Ok(line_prefix_o);
        for await chunk in inp_stream {
            match chunk {
                Err(e) => yield Err(e),
                Ok(chunk) => {
                    if chunk.contains(&b'\n') {
                        yield Ok(Bytes::copy_from_slice(&regex.replace_all(&chunk, line_prefix_n.as_bytes())));
                    } else {
                        yield Ok(chunk);
                    }
                }
            }
        }
    };
    StreamReader::new(oup_stream)
}

pub fn postproc_pagebreaks(line_prefix: &str, inp: impl AsyncRead) -> impl AsyncRead {
    let line_prefix = line_prefix.to_string(); // clone since
    let mut page_count = 1;

    panic!("todo!");
    tokio::io::empty()
    /*ByteReplacer {
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
    }*/
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use tokio::pin;
    use std::io::Read;

    async fn test_from_strs(pagebreaks: bool, line_prefix: &str, a: &'static str, b: &str) -> Result<()> {
        test_from_bytes(pagebreaks, line_prefix, a.as_bytes(), b).await
    }

    async fn test_from_bytes(pagebreaks: bool, line_prefix: &str, a: &'static [u8], b: &str) -> Result<()> {
        let mut oup = Vec::new();
        let inp = postproc_encoding("", a)?;
        if pagebreaks {
            postproc_pagebreaks(line_prefix, inp).read_to_end(&mut oup).await?;
        } else {
            let x = postproc_prefix(line_prefix, inp);
            pin!(x);
            x.read_to_end(&mut oup).await?;
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

    #[tokio::test]
    async fn post1() -> Result<()> {
        let inp = "What is this\nThis is a test\nFoo";
        let oup = "Page 1:What is this\nPage 1:This is a test\nPage 1:Foo";

        test_from_strs(true, "", inp, oup).await?;

        println!("\n\n\n\n");

        let inp = "What is this\nThis is a test\nFoo\x0c\nHelloooo\nHow are you?\x0c\nGreat!";
        let oup = "Page 1:What is this\nPage 1:This is a test\nPage 1:Foo\nPage 2:\nPage 2:Helloooo\nPage 2:How are you?\nPage 3:\nPage 3:Great!";

        test_from_strs(true, "", inp, oup).await?;

        let inp = "What is this\nThis is a test\nFoo\x0c\nHelloooo\nHow are you?\x0c\nGreat!";
        let oup = "foo.pdf:What is this\nfoo.pdf:This is a test\nfoo.pdf:Foo\x0c\nfoo.pdf:Helloooo\nfoo.pdf:How are you?\x0c\nfoo.pdf:Great!";

        test_from_strs(false, "foo.pdf:", inp, oup).await?;

        test_from_strs(
            false,
            "foo:",
            "this is a test \n\n \0 foo",
            "foo:[rga: binary data]",
        ).await?;
        test_from_strs(false, "foo:", "\0", "foo:[rga: binary data]").await?;

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
