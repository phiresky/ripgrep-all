use super::*;
use crate::print_bytes;
use anyhow::*;
use async_stream::stream;
use async_zip::base::read::seek::ZipFileReader;
use lazy_static::lazy_static;
use log::*;
use tokio::fs::File;
use tokio::io::BufReader;
use tokio_util::compat::TokioAsyncReadCompatExt;

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
            let file = File::open(&filepath_hint)
                .await
                .expect("Failed to open zip file");
            let archive = BufReader::new(file).compat();

            let mut reader = ZipFileReader::new(archive)
                .await
                .expect("Failed to read zip file");
            let s = stream! {
                for (i, entry) in reader.file().entries().iter().enumerate() {
                    // Skip directories.
                    if let async_zip::error::Result::Ok(is_dir) = entry.dir() {
                        if is_dir {
                            continue;
                        }
                    }
                    let filename = entry.filename();
                    let mut entry_reader = reader.reader_with_entry(i).await?;
                    if let async_zip::error::Result::Ok(printable) = filename.as_str() {
                        debug!(
                            "{}{}|{}: {} ({} packed)",
                            line_prefix,
                            filepath_hint.display(),
                            printable,
                            print_bytes(entry.uncompressed_size() as f64),
                            print_bytes(entry.compressed_size() as f64)
                        );
                        let new_line_prefix = format!("{}{}: ", line_prefix, printable);
                        let fname = PathBuf::from(printable);
                        tokio::pin!(entry_reader);
                        // SAFETY: this should be solvable without unsafe but idk how :(
                        // the issue is that ZipEntryReader borrows from ZipFileReader, but we need to yield it here into the stream
                        // but then it can't borrow from the ZipFile
                        let reader2 = unsafe {
                            std::intrinsics::transmute::<
                                Pin<&mut (dyn AsyncBufRead + Send)>,
                                Pin<&'static mut (dyn AsyncBufRead + Send)>,
                            >(entry_reader)
                        };
                        yield Ok(AdaptInfo {
                            filepath_hint: fname,
                            is_real_file: false,
                            inp: Box::pin(entry_reader),
                            line_prefix: new_line_prefix,
                            archive_recursion_depth: archive_recursion_depth + 1,
                            postprocess,
                            config: config.clone(),
                        });
                    }
                }
            };

            Ok(Box::pin(s))
        } else {
            let mut zip = ZipFileReader::new(inp);

            let s = stream! {
                    trace!("begin zip");
                    while let Some(mut entry) = zip.next_with_entry().await? {
                        trace!("zip next entry");
                        let file = entry.entry();
                        if file.filename().ends_with('/') {
                            zip = entry.skip().await?;

                            continue;
                        }
                        debug!(
                            "{}{}|{}: {} ({} packed)",
                            line_prefix,
                            filepath_hint.display(),
                            file.filename(),
                            print_bytes(file.uncompressed_size() as f64),
                            print_bytes(file.compressed_size() as f64)
                        );
                        let new_line_prefix = format!("{}{}: ", line_prefix, file.filename());
                        let fname = PathBuf::from(file.filename());
                        let reader = entry.reader();
                        tokio::pin!(reader);
                        // SAFETY: this should be solvable without unsafe but idk how :(
                        // the issue is that ZipEntryReader borrows from ZipFileReader, but we need to yield it here into the stream
                        // but then it can't borrow from the ZipFile
                        let reader2 = unsafe {
                            std::intrinsics::transmute::<
                                Pin<&mut (dyn AsyncBufRead + Send)>,
                                Pin<&'static mut (dyn AsyncBufRead + Send)>,
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
    use async_zip::base::write::ZipFileWriter;
    use async_zip::{Compression, ZipEntryBuilder};
    use tokio::io::BufWriter;

    use super::*;
    use crate::{preproc::loop_adapt, test_utils::*};
    use pretty_assertions::assert_eq;

    #[async_recursion::async_recursion]
    async fn create_zip(fname: &str, content: &str, add_inner: bool) -> Result<Vec<u8>> {
        let v: Vec<u8> = Vec::new();
        let mut cursor = std::io::Cursor::new(v);
        let mut writer = BufWriter::new(&mut cursor);
        let mut zip = ZipFileWriter::new(&mut writer);

        let options = ZipEntryBuilder::new(fname.to_string().into(), Compression::Stored);
        zip.write_entry_whole(options, content.as_bytes()).await?;

        if add_inner {
            let opts = ZipEntryBuilder::new("inner.zip".to_string().into(), Compression::Stored);
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
