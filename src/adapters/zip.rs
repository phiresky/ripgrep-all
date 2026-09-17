use super::*;
use crate::print_bytes;
use anyhow::*;
use async_stream::stream;
use lazy_static::lazy_static;
use log::*;
use std::task::{Context as TaskContext, Poll};
use tokio::io::{BufReader, ReadBuf};
use tokio::sync::oneshot;
use tokio_util::compat::{FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt};

type StreamingZipEntry = async_zip::base::read::stream::ZipFileReader<
    async_zip::tokio::read::stream::Reading<
        'static,
        BufReader<ReadBox>,
        async_zip::base::read::WithEntry<'static>,
    >,
>;

/// Own the archive input until this entry reaches EOF or its reader is dropped.
/// The iterator waits for ownership to return before opening another entry.
struct StreamingEntryReader {
    entry: Option<StreamingZipEntry>,
    return_entry: Option<oneshot::Sender<StreamingZipEntry>>,
}

impl StreamingEntryReader {
    fn return_entry(&mut self) {
        if let (Some(entry), Some(sender)) = (self.entry.take(), self.return_entry.take()) {
            // If the iterator was dropped, the entry and its input can be dropped too.
            let _ = sender.send(entry);
        }
    }
}

impl AsyncRead for StreamingEntryReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        // A zero-length read does not establish EOF.
        if buf.remaining() == 0 {
            return Poll::Ready(std::io::Result::Ok(()));
        }
        let Some(entry) = self.entry.as_mut() else {
            return Poll::Ready(std::io::Result::Ok(()));
        };
        let before = buf.filled().len();
        std::task::ready!(Pin::new(&mut entry.reader_mut().compat()).poll_read(cx, buf))?;
        if buf.filled().len() == before {
            self.return_entry();
        }
        Poll::Ready(std::io::Result::Ok(()))
    }
}

impl Drop for StreamingEntryReader {
    fn drop(&mut self) {
        self.return_entry();
    }
}

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
                    while let Some(entry) = zip.next_with_entry().await? {
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
                        let (sender, returned) = oneshot::channel();
                        let reader = StreamingEntryReader {
                            entry: Some(entry),
                            return_entry: Some(sender),
                        };
                        yield Ok(AdaptInfo {
                            filepath_hint: fname,
                            is_real_file: false,
                            inp: Box::pin(reader),
                            line_prefix: new_line_prefix,
                            archive_recursion_depth: archive_recursion_depth + 1,
                            postprocess,
                            config: config.clone(),
                        });
                        let entry = returned.await.context("ZIP entry reader did not return the archive input")?;
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

    #[tokio::test]
    async fn streaming_does_not_read_entire_entry_or_archive() -> Result<()> {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        use std::task::{Context, Poll};
        use tokio::io::{AsyncReadExt, ReadBuf};
        use tokio_stream::StreamExt;

        struct ReadBudget {
            input: std::io::Cursor<Vec<u8>>,
            limit: Arc<AtomicUsize>,
            consumed: Arc<AtomicUsize>,
        }

        impl AsyncRead for ReadBudget {
            fn poll_read(
                mut self: Pin<&mut Self>,
                _: &mut Context<'_>,
                buf: &mut ReadBuf<'_>,
            ) -> Poll<std::io::Result<()>> {
                if buf.remaining() == 0 {
                    return Poll::Ready(std::io::Result::Ok(()));
                }
                let available = self
                    .limit
                    .load(Ordering::Relaxed)
                    .saturating_sub(self.input.position() as usize);
                if available == 0 {
                    // Error rather than EOF: an eager read must fail, not appear complete.
                    return Poll::Ready(Err(std::io::Error::other(
                        "ZIP read ahead exceeded budget",
                    )));
                }
                let len = available.min(buf.remaining());
                let read = std::io::Read::read(&mut self.input, buf.initialize_unfilled_to(len))?;
                buf.advance(read);
                self.consumed.fetch_add(read, Ordering::Relaxed);
                Poll::Ready(std::io::Result::Ok(()))
            }
        }

        // Incompressible data ensures even the compressed archive exceeds the read budget.
        let mut state = 123456789_u32;
        let content: Vec<u8> = (0..1024 * 1024)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                state as u8
            })
            .collect();

        for compression in [Compression::Stored, Compression::Deflate] {
            let mut archive = std::io::Cursor::new(Vec::new());
            let mut writer = ZipFileWriter::with_tokio(&mut archive);
            writer
                .write_entry_whole(
                    ZipEntryBuilder::new("first.bin".into(), compression),
                    &content,
                )
                .await?;
            writer
                .write_entry_whole(
                    ZipEntryBuilder::new("second.txt".into(), compression),
                    b"second",
                )
                .await?;
            writer.close().await?;
            let archive = archive.into_inner();
            const BUDGET: usize = 32 * 1024;
            assert!(archive.len() > BUDGET * 16);
            let limit = Arc::new(AtomicUsize::new(BUDGET));
            let consumed = Arc::new(AtomicUsize::new(0));
            let input = ReadBudget {
                input: std::io::Cursor::new(archive),
                limit: limit.clone(),
                consumed: consumed.clone(),
            };
            let (ai, reason) = simple_adapt_info(&PathBuf::from("stream.zip"), Box::pin(input));
            let mut entries = ZipAdapter::new().adapt(ai, &reason).await?;
            let mut first = entries.next().await.context("missing first entry")??;
            assert_eq!(first.filepath_hint, PathBuf::from("first.bin"));
            let mut prefix = [0; 64];
            first.inp.read_exact(&mut prefix).await?;
            assert_eq!(prefix.as_slice(), &content[..prefix.len()]);
            let read = consumed.load(Ordering::Relaxed);
            assert!(
                read > 0 && read <= BUDGET,
                "read {read} bytes before returning a prefix"
            );

            // After the prefix is available, allow normal streaming to finish.
            limit.store(usize::MAX, Ordering::Relaxed);
            let remaining = tokio::io::copy(&mut first.inp, &mut tokio::io::sink()).await?;
            assert_eq!(remaining as usize, content.len() - prefix.len());
            drop(first);
            let mut second = entries.next().await.context("missing second entry")??;
            let mut text = String::new();
            second.inp.read_to_string(&mut text).await?;
            assert_eq!(text, "second");
            drop(second);
            assert!(entries.next().await.is_none());
            assert!(consumed.load(Ordering::Relaxed) > BUDGET);
        }
        Ok(())
    }

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
    async fn streaming_entry_ownership() -> Result<()> {
        use tokio::io::AsyncReadExt;
        use tokio_stream::StreamExt;

        let bytes = create_zip("first.txt", "first", true).await?;
        let (ai, reason) = simple_adapt_info(
            &PathBuf::from("stream.zip"),
            Box::pin(std::io::Cursor::new(bytes)),
        );
        let mut entries = ZipAdapter::new().adapt(ai, &reason).await?;
        let mut first = entries.next().await.unwrap()?;

        // Poll the actual reader with no capacity: this must not return archive ownership.
        let mut empty = [];
        let mut buf = ReadBuf::new(&mut empty);
        std::future::poll_fn(|cx| first.inp.as_mut().poll_read(cx, &mut buf)).await?;
        let mut next = tokio_test::task::spawn(entries.next());
        assert!(next.poll().is_pending());
        let mut text = String::new();
        first.inp.read_to_string(&mut text).await?;
        assert_eq!(text, "first");
        assert!(
            next.is_woken(),
            "EOF must wake the waiting archive iterator"
        );
        let mut second = tokio_test::assert_ready!(next.poll()).unwrap()?;
        drop(next);
        assert_eq!(second.filepath_hint, PathBuf::from("inner.zip"));

        // The exhausted reader can stay alive while the next entry is in use.
        assert_eq!(first.inp.read(&mut [0; 1]).await?, 0);
        // The live entry owns its input even after the archive iterator is gone.
        drop(entries);
        let mut inner = Vec::new();
        second.inp.read_to_end(&mut inner).await?;
        assert_eq!(
            inner,
            create_zip("inner.txt", "inner text file", false).await?
        );
        Ok(())
    }

    #[tokio::test]
    async fn dropping_unread_streaming_entry_reports_error() -> Result<()> {
        use tokio::io::AsyncReadExt;
        use tokio_stream::StreamExt;

        let bytes = create_zip("first.txt", "first", true).await?;
        let (ai, reason) = simple_adapt_info(
            &PathBuf::from("stream.zip"),
            Box::pin(std::io::Cursor::new(bytes)),
        );
        let mut entries = ZipAdapter::new().adapt(ai, &reason).await?;
        let mut first = entries.next().await.unwrap()?;
        first.inp.read_exact(&mut [0; 1]).await?;
        drop(first);
        let result =
            tokio::time::timeout(std::time::Duration::from_secs(1), entries.next()).await?;
        let error = result.unwrap().err().context("unread entry must fail")?;
        assert!(error.to_string().contains("entry was not read fully"));
        Ok(())
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
