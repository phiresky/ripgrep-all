use super::*;
use crate::print_bytes;
use anyhow::*;
use async_stream::stream;
use lazy_static::lazy_static;
use log::*;
use tokio_util::compat::{FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt};

// TODO: allow users to configure file extensions instead of hard coding the list
// https://github.com/phiresky/ripgrep-all/pull/208#issuecomment-2173241243
static EXTENSIONS: &[&str] = &["zip", "jar", "xpi", "kra", "snagx"];

lazy_static! {
    static ref METADATA: AdapterMeta = AdapterMeta {
        name: "zip".to_owned(),
        version: 1,
        description: "Reads a zip file as a stream and recurses down into its contents".to_owned(),
        recurses: true,
        fast_matchers: EXTENSIONS
            .iter()
            .map(|s| FastFileMatcher::FileExtension(s.to_string()))
            .collect(),
        slow_matchers: Some(vec![FileMatcher::MimeType("application/zip".to_owned())]),
        keep_fast_matchers_if_accurate: false,
        disabled_by_default: false
    };
}
#[derive(Default, Clone)]
pub struct ZipAdapter;

impl ZipAdapter {
    pub fn new() -> Self {
        Self
    }
}
impl GetMetadata for ZipAdapter {
    fn metadata(&self) -> &AdapterMeta {
        &METADATA
    }
}

#[async_trait]
impl FileAdapter for ZipAdapter {
    async fn adapt(
        &self,
        ai: AdaptInfo,
        _detection_reason: &FileMatcher,
    ) -> Result<AdaptedFilesIterBox> {
        // let (s, r) = mpsc::channel(1);
        let AdaptInfo {
            inp,
            filepath_hint,
            archive_recursion_depth,
            postprocess,
            line_prefix,
            config,
            is_real_file,
            ..
        } = ai;
        if is_real_file {
            use async_zip::base::read1::seek::ZipArchiveReader;

            let file = tokio::fs::File::open(&filepath_hint).await?;
            let zip = ZipArchiveReader::open(tokio::io::BufReader::new(file).compat()).await?;
            let inner = zip.inner().clone();
            drop(zip);
            let s = stream! {
                for (i, file) in inner.cdrs().iter().enumerate() {
                    let filename = std::str::from_utf8(file.insecure_file_name.as_bytes())?;
                    if filename.ends_with('/') {
                        continue;
                    }
                    let uncompressed_size = file.uncompressed_size()?;
                    let compressed_size = file.compressed_size()?;
                    debug!(
                        "{}{}|{}: {} ({} packed)",
                        line_prefix,
                        filepath_hint.display(),
                        filename,
                        print_bytes(uncompressed_size as f64),
                        print_bytes(compressed_size as f64)
                    );
                    let new_line_prefix = format!("{}{}: ", line_prefix, filename);
                    let fname = PathBuf::from(filename);
                    let source = tokio::fs::File::open(&filepath_hint).await?;
                    let zip = ZipArchiveReader::new_with_inner(
                        tokio::io::BufReader::new(source).compat(),
                        inner.clone(),
                    );
                    let reader = zip.file_oneshot(i).await?.compat();
                    yield Ok(AdaptInfo {
                        filepath_hint: fname,
                        is_real_file: false,
                        inp: Box::pin(reader),
                        line_prefix: new_line_prefix,
                        archive_recursion_depth: archive_recursion_depth + 1,
                        postprocess,
                        config: config.clone(),
                    });
                }
            };

            Ok(Box::pin(s))
        } else {
            use async_zip::base::read::stream::ZipFileReader;
            let mut zip = ZipFileReader::with_tokio(tokio::io::BufReader::new(inp));

            let s = stream! {
                    trace!("begin zip");
                    while let Some(mut entry) = zip.next_with_entry().await? {
                        trace!("zip next entry");
                        let file = entry.reader().entry();
                        let filename = file.filename().as_str()?;
                        if filename.ends_with('/') {
                            zip = entry.skip().await?;

                            continue;
                        }
                        debug!(
                            "{}{}|{}: {} ({} packed)",
                            line_prefix,
                            filepath_hint.display(),
                            filename,
                            print_bytes(file.uncompressed_size() as f64),
                            print_bytes(file.compressed_size() as f64)
                        );
                        let new_line_prefix = format!("{}{}: ", line_prefix, filename);
                        let fname = PathBuf::from(filename);
                        let reader = entry.reader_mut().compat();
                        tokio::pin!(reader);
                        // SAFETY: this should be solvable without unsafe but idk how :(
                        // the issue is that ZipEntryReader borrows from ZipFileReader, but we need to yield it here into the stream
                        // but then it can't borrow from the ZipFile
                        let reader2 = unsafe {
                            std::mem::transmute::<
                                Pin<&mut (dyn AsyncRead + Send)>,
                                Pin<&'static mut (dyn AsyncRead + Send)>,
                            >(reader)
                        };
                        yield Ok(AdaptInfo {
                            filepath_hint: fname,
                            is_real_file: false,
                            inp: Box::pin(reader2),
                            line_prefix: new_line_prefix,
                            archive_recursion_depth: archive_recursion_depth + 1,
                            postprocess,
                            config: config.clone(),
                        });
                        zip = entry.done().await.context("going to next file in zip but entry was not read fully")?;

                }
                trace!("zip over");
            };

            Ok(Box::pin(s))
        }
    }
}

/*struct ZipAdaptIter {
    inp: AdaptInfo,
}
impl<'a> AdaptedFilesIter for ZipAdaptIter<'a> {
    fn next<'b>(&'b mut self) -> Option<AdaptInfo<'b>> {
        let line_prefix = &self.inp.line_prefix;
        let filepath_hint = &self.inp.filepath_hint;
        let archive_recursion_depth = &self.inp.archive_recursion_depth;
        let postprocess = self.inp.postprocess;
        ::zip::read::read_zipfile_from_stream(&mut self.inp.inp)
            .unwrap()
            .and_then(|file| {
                if file.is_dir() {
                    return None;
                }
                debug!(
                    "{}{}|{}: {} ({} packed)",
                    line_prefix,
                    filepath_hint.to_string_lossy(),
                    file.name(),
                    print_bytes(file.size() as f64),
                    print_bytes(file.compressed_size() as f64)
                );
                let line_prefix = format!("{}{}: ", line_prefix, file.name());
                Some(AdaptInfo {
                    filepath_hint: PathBuf::from(file.name()),
                    is_real_file: false,
                    inp: Box::new(file),
                    line_prefix,
                    archive_recursion_depth: archive_recursion_depth + 1,
                    postprocess,
                    config: RgaConfig::default(), //config.clone(),
                })
            })
    }
}*/

#[cfg(test)]
mod test {
    use async_zip::{Compression, ZipEntryBuilder, base::write::ZipFileWriter};

    use super::*;
    use crate::{preproc::loop_adapt, test_utils::*};
    use pretty_assertions::assert_eq;

    #[async_recursion::async_recursion]
    async fn create_zip(fname: &str, content: &str, add_inner: bool) -> Result<Vec<u8>> {
        let v = Vec::new();
        let mut cursor = std::io::Cursor::new(v);
        let mut zip = ZipFileWriter::with_tokio(&mut cursor);

        let options = ZipEntryBuilder::new(fname.into(), Compression::Stored);
        zip.write_entry_whole(options, content.as_bytes()).await?;

        if add_inner {
            let opts = ZipEntryBuilder::new("inner.zip".into(), Compression::Stored);
            zip.write_entry_whole(
                opts,
                &create_zip("inner.txt", "inner text file", false).await?,
            )
            .await?;
        }
        zip.close().await?;
        Ok(cursor.into_inner())
    }

    #[tokio::test]
    async fn directories_and_compressed_files() -> Result<()> {
        let mut cursor = std::io::Cursor::new(Vec::new());
        let mut writer = ZipFileWriter::with_tokio(&mut cursor);
        for (name, content) in [
            ("dir/", ""),
            ("dir/first.txt", "first"),
            ("second.txt", "second"),
        ] {
            writer
                .write_entry_whole(
                    ZipEntryBuilder::new(name.into(), Compression::Deflate),
                    content.as_bytes(),
                )
                .await?;
        }
        writer.close().await?;
        let bytes = cursor.into_inner();
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("test.zip");
        tokio::fs::write(&path, &bytes).await?;
        for is_real_file in [false, true] {
            let (mut ai, reason) =
                simple_adapt_info(&path, Box::pin(std::io::Cursor::new(bytes.clone())));
            ai.is_real_file = is_real_file;
            let output = adapted_to_vec(loop_adapt(&ZipAdapter::new(), reason, ai).await?).await?;
            assert_eq!(
                String::from_utf8(output)?,
                "PREFIX:dir/first.txt: first\nPREFIX:second.txt: second\n"
            );
        }
        Ok(())
    }

    #[tokio::test]
    async fn corrupt_file_is_rejected() -> Result<()> {
        let mut bytes = create_zip("file.txt", "original content", false).await?;
        let offset = bytes
            .windows(16)
            .position(|w| w == b"original content")
            .unwrap();
        bytes[offset] = b'X';
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("corrupt.zip");
        tokio::fs::write(&path, bytes).await?;
        let (ai, reason) = simple_fs_adapt_info(&path).await?;
        let result = adapted_to_vec(loop_adapt(&ZipAdapter::new(), reason, ai).await?).await;
        assert!(
            result.is_err(),
            "corrupted ZIP content must fail validation"
        );
        Ok(())
    }

    #[tokio::test]
    async fn only_seek_zip_fs() -> Result<()> {
        let zip = test_data_dir().join("only-seek-zip.zip");
        let (a, d) = simple_fs_adapt_info(&zip).await?;
        let _v = adapted_to_vec(loop_adapt(&ZipAdapter::new(), d, a).await?).await?;
        // assert_eq!(String::from_utf8(v)?, "");

        Ok(())
    }
    /*#[tokio::test]
    async fn only_seek_zip_mem() -> Result<()> {
        let zip = test_data_dir().join("only-seek-zip.zip");
        let (a, d) = simple_adapt_info(&zip, Box::pin(File::open(&zip).await?));
        let v = adapted_to_vec(loop_adapt(&ZipAdapter::new(), d, a)?).await?;
        // assert_eq!(String::from_utf8(v)?, "");

        Ok(())
    }*/
    #[tokio::test]
    async fn recurse() -> Result<()> {
        let zipfile = create_zip("outer.txt", "outer text file", true).await?;
        let adapter = ZipAdapter::new();

        let (a, d) = simple_adapt_info(
            &PathBuf::from("outer.zip"),
            Box::pin(std::io::Cursor::new(zipfile)),
        );
        let buf = adapted_to_vec(loop_adapt(&adapter, d, a).await?).await?;

        assert_eq!(
            String::from_utf8(buf)?,
            "PREFIX:outer.txt: outer text file\nPREFIX:inner.zip: inner.txt: inner text file\n",
        );

        Ok(())
    }
}
