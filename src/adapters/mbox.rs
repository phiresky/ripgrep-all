use crate::adapted_iter::one_file;

use super::*;

use anyhow::Result;
use async_stream::stream;
use lazy_static::lazy_static;
use tokio::io::{BufReader, AsyncReadExt};

use std::{path::{Path, PathBuf}, sync::Mutex, io::Cursor};

static EXTENSIONS: &[&str] = &["mbox", "mbx"];
static MIME_TYPES: &[&str] = &[
    "application/mbox",
];
lazy_static! {
    static ref METADATA: AdapterMeta = AdapterMeta {
        name: "mbox".to_owned(),
        version: 1,
        description:
            "Reads mailbox files and runs extractors on the contents and attachments."
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
}
#[derive(Default)]
pub struct MboxAdapter;

impl MboxAdapter {
    pub fn new() -> MboxAdapter {
        MboxAdapter
    }
}
impl GetMetadata for MboxAdapter {
    fn metadata(&self) -> &AdapterMeta {
        &METADATA
    }
}

fn get_inner_filename(filename: &Path) -> PathBuf {
    let extension = filename
        .extension()
        .map(|e| e.to_string_lossy())
        .unwrap_or(Cow::Borrowed(""));
    let stem = filename
        .file_stem()
        .expect("no filename given?")
        .to_string_lossy();
    let new_extension = match extension.as_ref() {
        "tgz" | "tbz" | "tbz2" => ".tar",
        _other => "",
    };
    filename.with_file_name(format!("{}{}", stem, new_extension))
}

impl FileAdapter for MboxAdapter {
    fn adapt(&self, ai: AdaptInfo, _detection_reason: &FileMatcher) -> Result<AdaptedFilesIterBox> {
        println!("running mbox adapter");
        let AdaptInfo {
            filepath_hint,
            mut inp,
            line_prefix,
            archive_recursion_depth,
            config,
            postprocess,
            ..
        } = ai;

        let mut content = String::new();
        let s = stream! {
            inp.read_to_string(&mut content).await?;

            let mut ais = vec![];
            for mail in content.split("\nFrom ") {

                let mail_bytes = mail.as_bytes(); // &content[offset..offset2];
                let mail_content = mail_bytes.splitn(2, |x| *x == b'\n').skip(1).next().unwrap();
                let mail = mailparse::parse_mail(mail_content)?;
                let mail_body = mail.get_body()?;
                println!("body {:?}", mail_body);

                let mut path = filepath_hint.clone();
                println!("{:?}", mail.ctype.mimetype);
                match &*mail.ctype.mimetype {
                    "text/html" => {
                        path.push("mail.html");
                    },
                    _ => {
                        path.push("mail.txt");
                    }
                }

                let mut config = config.clone();
                config.accurate = true;

                let ai2: AdaptInfo = AdaptInfo {
                    filepath_hint: path,
                    is_real_file: false,
                    archive_recursion_depth: archive_recursion_depth + 1,
                    inp: Box::pin(Cursor::new(mail_body.into_bytes())),
                    line_prefix: line_prefix.to_string(),
                    config: config,
                    postprocess,
                };
                ais.push(ai2);
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

    #[test]
    fn test_inner_filename() {
        for (a, b) in &[
            ("hi/test.tgz", "hi/test.tar"),
            ("hi/hello.gz", "hi/hello"),
            ("a/b/initramfs", "a/b/initramfs"),
            ("hi/test.tbz2", "hi/test.tar"),
            ("hi/test.tbz", "hi/test.tar"),
            ("hi/test.hi.bz2", "hi/test.hi"),
            ("hello.tar.gz", "hello.tar"),
        ] {
            assert_eq!(get_inner_filename(&PathBuf::from(a)), PathBuf::from(*b));
        }
    }

    #[tokio::test]
    async fn gz() -> Result<()> {
        let adapter = MboxAdapter;

        let filepath = test_data_dir().join("hello.gz");

        let (a, d) = simple_adapt_info(&filepath, Box::pin(File::open(&filepath).await?));
        let r = adapter.adapt(a, &d)?;
        let o = adapted_to_vec(r).await?;
        assert_eq!(String::from_utf8(o)?, "hello\n");
        Ok(())
    }

    #[tokio::test]
    async fn pdf_gz() -> Result<()> {
        let adapter = MboxAdapter;

        let filepath = test_data_dir().join("short.pdf.gz");

        let (a, d) = simple_adapt_info(&filepath, Box::pin(File::open(&filepath).await?));
        let r = loop_adapt(&adapter, d, a)?;
        let o = adapted_to_vec(r).await?;
        assert_eq!(
            String::from_utf8(o)?,
            "PREFIX:Page 1: hello world
PREFIX:Page 1: this is just a test.
PREFIX:Page 1: 
PREFIX:Page 1: 1
PREFIX:Page 1: 
PREFIX:Page 1: 
"
        );
        Ok(())
    }
}
