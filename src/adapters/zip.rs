use super::*;
use crate::print_bytes;
use anyhow::*;
use async_stream::stream;
use lazy_static::lazy_static;
use log::*;
<<<<<<< HEAD
=======
use tokio::io::AsyncRead;
>>>>>>> 63d8839 (perf(zip): enable zero-copy operations for better performance)

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
            use async_zip::read::fs::ZipFileReader;

            let zip = ZipFileReader::new(&filepath_hint).await?;
            let s = stream! {
                for i in 0..zip.file().entries().len() {
                    let file = zip.get_entry(i)?;
                    let reader = zip.entry(i).await?;
                    if file.filename().ends_with('/') {
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
                    tokio::pin!(reader);
<<<<<<< HEAD
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
=======
                    let boxed = entry_readbox(reader);
                    yield Ok(AdaptInfo {
                        filepath_hint: fname,
                        is_real_file: false,
                        inp: boxed,
>>>>>>> 63d8839 (perf(zip): enable zero-copy operations for better performance)
                        line_prefix: new_line_prefix,
                        archive_recursion_depth: archive_recursion_depth + 1,
                        postprocess,
                        config: config.clone(),
                    });
                }
            };

            Ok(Box::pin(s))
        } else {
            use async_zip::read::stream::ZipFileReader;
            let mut zip = ZipFileReader::new(inp);

            let s = stream! {
                    trace!("begin zip");
                    while let Some(mut entry) = zip.next_entry().await? {
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
<<<<<<< HEAD
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
=======
                        let boxed = entry_readbox(reader);
                        yield Ok(AdaptInfo {
                            filepath_hint: fname,
                            is_real_file: false,
                            inp: boxed,
>>>>>>> 63d8839 (perf(zip): enable zero-copy operations for better performance)
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
    use async_zip::{Compression, ZipEntryBuilder, write::ZipFileWriter};

    use super::*;
    use crate::{preproc::loop_adapt, test_utils::*};
    use pretty_assertions::assert_eq;

    #[async_recursion::async_recursion]
    async fn create_zip(fname: &str, content: &str, add_inner: bool) -> Result<Vec<u8>> {
        let v = Vec::new();
        let mut cursor = std::io::Cursor::new(v);
        let mut zip = ZipFileWriter::new(&mut cursor);

        let options = ZipEntryBuilder::new(fname.to_string(), Compression::Stored);
        zip.write_entry_whole(options, content.as_bytes()).await?;

        if add_inner {
            let opts = ZipEntryBuilder::new("inner.zip".to_string(), Compression::Stored);
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
<<<<<<< HEAD
        let _v = adapted_to_vec(loop_adapt(&ZipAdapter::new(), d, a).await?).await?;
=======
        let engine = crate::preproc::make_engine(&a.config)?;
        let _v = adapted_to_vec(loop_adapt(&engine, &ZipAdapter::new(), d, a).await?).await?;
>>>>>>> 63d8839 (perf(zip): enable zero-copy operations for better performance)
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
<<<<<<< HEAD
        let buf = adapted_to_vec(loop_adapt(&adapter, d, a).await?).await?;
=======
        let engine = crate::preproc::make_engine(&a.config)?;
        let buf = adapted_to_vec(loop_adapt(&engine, &adapter, d, a).await?).await?;
>>>>>>> 63d8839 (perf(zip): enable zero-copy operations for better performance)

        assert_eq!(
            String::from_utf8(buf)?,
            "PREFIX:outer.txt: outer text file\nPREFIX:inner.zip: inner.txt: inner text file\n",
        );

        Ok(())
    }
}
<<<<<<< HEAD
=======
fn entry_readbox(reader: Pin<&mut (dyn AsyncRead + Send)>) -> ReadBox {
    // SAFETY: The returned Pin<&'static mut _> is only used within the yielded AdaptInfo lifetime.
    // The underlying Zip reader remains alive until the stream yields, so transmute is confined here.
    let reader2 = unsafe {
        std::mem::transmute::<
            Pin<&mut (dyn AsyncRead + Send)>,
            Pin<&'static mut (dyn AsyncRead + Send)>,
        >(reader)
    };
    Box::pin(reader2)
}
>>>>>>> 63d8839 (perf(zip): enable zero-copy operations for better performance)
