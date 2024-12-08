use super::*;

use anyhow::Result;
use async_stream::stream;
use lazy_static::lazy_static;
use mime2ext::mime2ext;
use regex::bytes::Regex;
use tokio::io::AsyncReadExt;

use std::{collections::VecDeque, io::Cursor};

pub const EXTENSIONS: &[&str] = &["mbox", "mbx", "eml"];
pub const MIMETYPES: &[&str] = &["application/mbox", "message/rfc822"];

lazy_static! {
    static ref FROM_REGEX: Regex = Regex::new("\r?\nFrom [^\n]+\n").unwrap();
}

#[derive(Default)]
pub struct MboxAdapter {
    pub extensions: Vec<String>,
    pub mimetypes: Vec<String>,
}

impl Adapter for MboxAdapter {
    fn name(&self) -> String {
        String::from("mail")
    }
    fn version(&self) -> i32 {
        1
    }
    fn description(&self) -> String {
        String::from(
            "Reads mailbox/mail files and runs extractors on the contents and attachments.",
        )
    }
    fn recurses(&self) -> bool {
        true
    }
    fn disabled_by_default(&self) -> bool {
        false
    }
    fn keep_fast_matchers_if_accurate(&self) -> bool {
        true
    }
    fn extensions(&self) -> Vec<String> {
        self.extensions.clone()
    }
    fn mimetypes(&self) -> Vec<String> {
        self.mimetypes.clone()
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
            mut inp,
            line_prefix,
            archive_recursion_depth,
            config,
            postprocess,
            ..
        } = ai;

        let mut content = Vec::new();
        let s = stream! {
            inp.read_to_end(&mut content).await?;

            let mut ais = vec![];
            for mail_bytes in FROM_REGEX.splitn(&content, usize::MAX) {
                let mail_content = mail_bytes.splitn(2, |x| *x == b'\n').nth(1).unwrap();
                let mail = mailparse::parse_mail(mail_content);
                if mail.is_err() {
                    continue;
                }
                let mail = mail.unwrap();

                let mut todos = VecDeque::new();
                todos.push_back(mail);

                while let Some(mail) = todos.pop_front() {
                let mut path = filepath_hint.clone();
                let filename = mail.get_content_disposition().params.get("filename").cloned();
                match &*mail.ctype.mimetype {
                    x if x.starts_with("multipart/") => {
                        todos.extend(mail.subparts);
                        continue;
                    }
                    mime => {
                        if let Some(name) = filename {
                            path.push(name);
                        } else if let Some(extension) = mime2ext(mime) {
                            path.push(format!("data.{extension}"));
                        } else {
                            path.push("data");
                        }
                    }
                }

                let mut config = config.clone();
                config.accurate = true;

                let raw_body = mail.get_body_raw();
                if raw_body.is_err() {
                    continue;
                }
                let ai2: AdaptInfo = AdaptInfo {
                    filepath_hint: path,
                    is_real_file: false,
                    archive_recursion_depth: archive_recursion_depth + 1,
                    inp: Box::pin(Cursor::new(raw_body.unwrap())),
                    line_prefix: line_prefix.to_string(),
                    config,
                    postprocess,
                };
                ais.push(ai2);
                }
            }
            for a in ais {
                yield(Ok(a));
            }
        };
        Ok(Box::pin(s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preproc::loop_adapt;
    use crate::test_utils::*;
    use pretty_assertions::assert_eq;
    use tokio::fs::File;
    use tokio_stream::StreamExt;

    #[tokio::test]
    async fn mail_simple() -> Result<()> {
        let adapter = MboxAdapter::default();

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
                .last()
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
        let adapter = MboxAdapter::default();

        let filepath = test_data_dir().join("test.mbx");

        let (a, d) = simple_adapt_info(&filepath, Box::pin(File::open(&filepath).await?));
        let mut r = adapter.adapt(a, &d).await?;
        let mut count = 0;
        while let Some(file) = r.next().await {
            let mut file = file?;
            assert_eq!(
                "data.html",
                file.filepath_hint.components().last().unwrap().as_os_str()
            );
            let mut buf = Vec::new();
            file.inp.read_to_end(&mut buf).await?;
            assert_eq!("<html>\r\n  <head>\r\n    <meta http-equiv=\"content-type\" content=\"text/html; charset=UTF-8\">\r\n  </head>\r\n  <body>\r\n    <p>&gt;From</p>\r\n    <p>Another word &gt;From<br>\r\n    </p>\r\n  </body>\r\n</html>", String::from_utf8(buf)?.trim());
            count += 1;
        }
        assert_eq!(3, count);
        Ok(())
    }

    #[tokio::test]
    async fn mbox_attachment() -> Result<()> {
        init_logging();

        let adapter = MboxAdapter::default();

        let filepath = test_data_dir().join("mail_with_attachment.mbox");

        let (a, d) = simple_adapt_info(&filepath, Box::pin(File::open(&filepath).await?));
        let mut r = loop_adapt(&adapter, d, a).await?;
        let mut count = 0;
        while let Some(file) = r.next().await {
            let mut file = file?;
            let path = file
                .filepath_hint
                .components()
                .last()
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
                    assert_eq!("PREFIX:Page 1: hello world\nPREFIX:Page 1: this is just a test.\nPREFIX:Page 1: \nPREFIX:Page 1: 1\nPREFIX:Page 1: \nPREFIX:Page 1: \n", String::from_utf8(buf).unwrap_or("err".to_owned()));
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
