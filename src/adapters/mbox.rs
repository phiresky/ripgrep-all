use super::*;

use anyhow::Result;
use async_stream::stream;
use lazy_static::lazy_static;
use mime2ext::mime2ext;
use regex::bytes::Regex;
use tokio_util::io::ReaderStream;
use tokio_stream::StreamExt;

use std::{collections::VecDeque, io::Cursor};

static EXTENSIONS: &[&str] = &["mbox", "mbx", "eml"];
static MIME_TYPES: &[&str] = &["application/mbox", "message/rfc822"];
lazy_static! {
    static ref METADATA: AdapterMeta = AdapterMeta {
        name: "mail".to_owned(),
        version: 1,
        description:
            "Reads mailbox/mail files and runs extractors on the contents and attachments."
                .to_owned(),
        recurses: true,
        fast_matchers: EXTENSIONS
            .iter()
            .map(|s| FastFileMatcher::FileExtension(s.to_string()))
            .collect(),
        slow_matchers: Some(
            MIME_TYPES
                .iter()
                .map(|s| FileMatcher::MimeType(s.to_string()))
                .collect()
        ),
        disabled_by_default: true,
        keep_fast_matchers_if_accurate: true
    };
    static ref FROM_REGEX: Regex = Regex::new("\r?\nFrom [^\n]+\n").unwrap();
}
#[derive(Default)]
pub struct MboxAdapter;

impl MboxAdapter {
    pub fn new() -> Self {
        Self
    }
}
impl GetMetadata for MboxAdapter {
    fn metadata(&self) -> &AdapterMeta {
        &METADATA
    }
}

#[async_trait]
impl FileAdapter for MboxAdapter {
    async fn adapt(
        &self,
        ai: AdaptInfo,
        _detection_reason: &FileMatcher,
    ) -> Result<AdaptedFilesIterBox> {
        let AdaptInfo {
            filepath_hint,
            inp,
            line_prefix,
            archive_recursion_depth,
            config,
            postprocess,
            ..
        } = ai;

        let s = stream! {
            let mut buffer: Vec<u8> = Vec::new();
            let mut stream = ReaderStream::new(inp);
            let mut scan_from: usize = 0;
            while let Some(chunk) = stream.next().await {
                let chunk = chunk?;
                let _old_len = buffer.len();
                buffer.extend_from_slice(&chunk);
                let data = &buffer[..];
                let mut indices: Vec<usize> = Vec::new();
                let mut pos = scan_from.saturating_sub(6);
                while let Some(nl_off) = memchr::memchr(b'\n', &data[pos..]) {
                    let i = pos + nl_off;
                    let end = i + 6;
                    if end <= data.len() && &data[i+1..end] == b"From " { indices.push(i+1); }
                    pos = i + 1;
                }
                scan_from = buffer.len();
                if !indices.is_empty() {
                    for w in indices.windows(2) {
                        let a = w[0];
                        let b = w[1];
                        let mut msg = &data[a..b];
                        if let Some(p) = memchr::memchr(b'\n', msg) { msg = &msg[p+1..]; }
                        if msg.is_empty() { continue; }
                        if let Ok(mail) = mailparse::parse_mail(msg) {
                            let mut todos = VecDeque::new();
                            todos.push_back(mail);
                            while let Some(mail) = todos.pop_front() {
                                let mut path = filepath_hint.clone();
                                let filename = mail.get_content_disposition().params.get("filename").cloned();
                                match &*mail.ctype.mimetype {
                                    x if x.starts_with("multipart/") => { todos.extend(mail.subparts); continue; }
                                    mime => {
                                        if let Some(name) = filename { path.push(name); }
                                        else if let Some(extension) = mime2ext(mime) { path.push(format!("data.{extension}")); }
                                        else { path.push("data"); }
                                    }
                                }
                                let mut cfg = config.clone();
                                cfg.accurate = true;
                                if let Ok(body) = mail.get_body_raw() {
                                    let ai2 = AdaptInfo {
                                        filepath_hint: path,
                                        is_real_file: false,
                                        archive_recursion_depth: archive_recursion_depth + 1,
                                        inp: Box::pin(Cursor::new(body)),
                                        line_prefix: line_prefix.to_string(),
                                        config: cfg,
                                        postprocess,
                                    };
                                    yield Ok(ai2);
                                }
                            }
                        }
                    }
                    let last = *indices.last().unwrap();
                    buffer = buffer.split_off(last);
                    scan_from = buffer.len();
                }
            }
            if !buffer.is_empty() {
                let msg = &buffer[..];
                if let Ok(mail) = mailparse::parse_mail(msg) {
                    let mut todos = VecDeque::new();
                    todos.push_back(mail);
                    while let Some(mail) = todos.pop_front() {
                        let mut path = filepath_hint.clone();
                        let filename = mail.get_content_disposition().params.get("filename").cloned();
                        match &*mail.ctype.mimetype {
                            x if x.starts_with("multipart/") => { todos.extend(mail.subparts); continue; }
                            mime => {
                                if let Some(name) = filename { path.push(name); }
                                else if let Some(extension) = mime2ext(mime) { path.push(format!("data.{extension}")); }
                                else { path.push("data"); }
                            }
                        }
                        let mut cfg = config.clone();
                        cfg.accurate = true;
                        if let Ok(body) = mail.get_body_raw() {
                            let ai2 = AdaptInfo {
                                filepath_hint: path,
                                is_real_file: false,
                                archive_recursion_depth: archive_recursion_depth + 1,
                                inp: Box::pin(Cursor::new(body)),
                                line_prefix: line_prefix.to_string(),
                                config: cfg,
                                postprocess,
                            };
                            yield Ok(ai2);
                        }
                    }
                }
            }
        };
        Ok(Box::pin(s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;
    use crate::preproc::loop_adapt;
    use crate::test_utils::*;
    use pretty_assertions::assert_eq;
    use tokio::fs::File;
    use tokio_stream::StreamExt;

    #[tokio::test]
    async fn mail_simple() -> Result<()> {
        let adapter = MboxAdapter;

        let filepath = test_data_dir().join("github_email.eml");

        let (a, d) = simple_adapt_info(&filepath, Box::pin(File::open(&filepath).await?));
        let mut r = adapter.adapt(a, &d).await?;
        let mut count = 0;
        while let Some(file) = r.next().await {
            let mut file = file?;
            let mut buf = Vec::new();
            file.inp.read_to_end(&mut buf).await?;
            match file
                .filepath_hint
                .components()
                .next_back()
                .unwrap()
                .as_os_str()
                .to_str()
                .unwrap()
            {
                "data.txt" | "data.html" => {
                    assert!(String::from_utf8(buf)?.contains("Thank you for your contribution"));
                }
                x => panic!("unexpected filename {x:?}"),
            }
            count += 1;
        }
        assert_eq!(2, count);
        Ok(())
    }

    #[tokio::test]
    async fn mbox_simple() -> Result<()> {
        let adapter = MboxAdapter;

        let filepath = test_data_dir().join("test.mbx");

        let (a, d) = simple_adapt_info(&filepath, Box::pin(File::open(&filepath).await?));
        let mut r = adapter.adapt(a, &d).await?;
        let mut count = 0;
        while let Some(file) = r.next().await {
            let mut file = file?;
            assert_eq!(
                "data.html",
                file.filepath_hint
                    .components()
                    .next_back()
                    .unwrap()
                    .as_os_str()
            );
            let mut buf = Vec::new();
            file.inp.read_to_end(&mut buf).await?;
            assert_eq!(
                "<html>\r\n  <head>\r\n    <meta http-equiv=\"content-type\" content=\"text/html; charset=UTF-8\">\r\n  </head>\r\n  <body>\r\n    <p>&gt;From</p>\r\n    <p>Another word &gt;From<br>\r\n    </p>\r\n  </body>\r\n</html>",
                String::from_utf8(buf)?.trim()
            );
            count += 1;
        }
        assert_eq!(3, count);
        Ok(())
    }

    #[tokio::test]
    async fn mbox_attachment() -> Result<()> {
        init_logging();

        let adapter = MboxAdapter;

        let filepath = test_data_dir().join("mail_with_attachment.mbox");

        let (a, d) = simple_adapt_info(&filepath, Box::pin(File::open(&filepath).await?));
        let engine = crate::preproc::make_engine(&a.config)?;
        let mut r = loop_adapt(engine, &adapter, d, a).await?;
        let mut count = 0;
        while let Some(file) = r.next().await {
            let mut file = file?;
            let path = file
                .filepath_hint
                .components()
                .next_back()
                .unwrap()
                .as_os_str()
                .to_str()
                .unwrap();
            let mut buf = Vec::new();
            file.inp.read_to_end(&mut buf).await?;
            match path {
                "data.html.txt" => {
                    assert_eq!(
                        "PREFIX:regular text\nPREFIX:\n",
                        String::from_utf8(buf).unwrap_or("err".to_owned())
                    );
                }
                "short.pdf.txt" => {
                    assert_eq!(
                        "PREFIX:Page 1: hello world\nPREFIX:Page 1: this is just a test.\nPREFIX:Page 1: \nPREFIX:Page 1: 1\nPREFIX:Page 1: \nPREFIX:Page 1: \n",
                        String::from_utf8(buf).unwrap_or("err".to_owned())
                    );
                }
                _ => {
                    panic!("unrelated {path:?}");
                }
            }
            count += 1;
        }
        assert_eq!(2, count); // one message + one attachment
        Ok(())
    }
}
