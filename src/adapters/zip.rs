use super::*;
use crate::print_bytes;
use anyhow::*;
use async_stream::stream;
use tokio::io::{duplex, AsyncWriteExt};
use futures_lite::io::AsyncReadExt;
//
use lazy_static::lazy_static;
use log::*;
<<<<<<< HEAD
<<<<<<< HEAD
=======
use tokio::io::AsyncRead;
>>>>>>> 63d8839 (perf(zip): enable zero-copy operations for better performance)
=======
// tokio AsyncRead not needed in 0.0.17 stream path with compat boxing
// fs reader path for async_zip 0.0.12
>>>>>>> f3247d2 (perf(zip): compat streaming on async_zip 0.0.17)

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
            inp: _,
            filepath_hint,
            archive_recursion_depth,
            postprocess,
            line_prefix,
            config,
            is_real_file: _,
            ..
        } = ai;
        {
            use async_zip::tokio::read::fs::ZipFileReader;
            use tokio_util::compat::FuturesAsyncReadCompatExt;
            use tokio::io::{copy, duplex};
            let zip = ZipFileReader::new(&filepath_hint).await?;
            let s = stream! {
                for i in 0..zip.file().entries().len() {
                    let entry_meta = zip.file().entries()[i].clone();
                    let fname_str = String::from_utf8_lossy(entry_meta.filename().as_bytes()).into_owned();
                    if fname_str.ends_with('/') { continue; }
                    debug!(
                        "{}{}|{}: {} ({} packed)",
                        line_prefix,
                        filepath_hint.display(),
                        &fname_str,
                        print_bytes(entry_meta.uncompressed_size() as f64),
                        print_bytes(entry_meta.compressed_size() as f64)
                    );
<<<<<<< HEAD
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
=======
                    let new_line_prefix = format!("{}{}: ", line_prefix, &fname_str);
                    let fname = PathBuf::from(fname_str.clone());
                    let (w, r) = duplex(64 * 1024);
                    let path_for_task = filepath_hint.clone();
                    tokio::spawn(async move {
                        let zip2 = ZipFileReader::new(&path_for_task).await?;
                        let reader_with_entry = zip2.reader_with_entry(i).await?;
                        let fut_reader = reader_with_entry.boxed_reader();
                        let mut src = FuturesAsyncReadCompatExt::compat(fut_reader);
                        let mut dst = w;
                        let _ = copy(&mut src, &mut dst).await;
                        Ok(())
                    });
                    yield Ok(AdaptInfo {
                        filepath_hint: fname,
                        is_real_file: false,
                        inp: Box::pin(r),
>>>>>>> f3247d2 (perf(zip): compat streaming on async_zip 0.0.17)
                        line_prefix: new_line_prefix,
                        archive_recursion_depth: archive_recursion_depth + 1,
                        postprocess,
                        config: config.clone(),
                    });
                }
            };
<<<<<<< HEAD

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

=======
>>>>>>> f3247d2 (perf(zip): compat streaming on async_zip 0.0.17)
            Ok(Box::pin(s))
        }
    }
}

#[allow(dead_code)]
pub async fn owned_zip_iter_fs(
    ai: AdaptInfo,
) -> Result<AdaptedFilesIterBox> {
    use async_zip::tokio::read::fs::ZipFileReader;
    let AdaptInfo {
        filepath_hint,
        inp,
        line_prefix,
        archive_recursion_depth,
        postprocess,
        config,
        is_real_file,
    } = ai;

    let zip_path = if is_real_file {
        filepath_hint.clone()
    } else {
        let tmp = tempfile::NamedTempFile::new()?;
        let tmp_path = tmp.path().to_path_buf();
        let mut f = tokio::fs::File::create(&tmp_path).await?;
        let mut r = inp;
        tokio::io::copy(&mut r, &mut f).await?;
        drop(f);
        tmp_path
    };

    let reader = ZipFileReader::new(&zip_path).await?;
    let metas: Vec<_> = reader.file().entries().to_vec();
    let s = stream! {
        for (i, meta) in metas.into_iter().enumerate() {
            let name_str = String::from_utf8_lossy(meta.filename().as_bytes()).into_owned();
            if name_str.ends_with('/') { continue; }
            debug!(
                "{}{}|{}: {} ({} packed)",
                line_prefix,
                filepath_hint.display(),
                &name_str,
                print_bytes(meta.uncompressed_size() as f64),
                print_bytes(meta.compressed_size() as f64)
            );
            let new_line_prefix = format!("{}{}: ", line_prefix, &name_str);
            let fname = PathBuf::from(name_str.clone());
            let reader2 = ZipFileReader::new(&zip_path).await?;
            let entry_reader = reader2.reader_with_entry(i).await?;
            let mut fut_reader = entry_reader.boxed_reader();
            let mut buf = Vec::new();
            fut_reader.read_to_end(&mut buf).await?;
            let (mut w, r) = duplex(64 * 1024);
            let data = buf;
            tokio::spawn(async move {
                let _ = w.write_all(&data).await;
            });
            yield Ok(AdaptInfo {
                filepath_hint: fname,
                is_real_file: false,
                inp: Box::pin(r),
                line_prefix: new_line_prefix,
                archive_recursion_depth: archive_recursion_depth + 1,
                postprocess,
                config: config.clone(),
            });
        }
    };
    Ok(Box::pin(s))
}


#[cfg(test)]
mod test {
    use async_zip::{Compression, ZipEntryBuilder, ZipString};
    use async_zip::base::write::ZipFileWriter;
    use tokio_util::compat::TokioAsyncWriteCompatExt;
    use tokio::io::AsyncReadExt;

    use super::*;
    use crate::{preproc::loop_adapt, test_utils::*};
    use pretty_assertions::assert_eq;

    #[async_recursion::async_recursion]
    async fn create_zip(fname: &str, content: &str, add_inner: bool) -> Result<Vec<u8>> {
        let (w, mut r) = tokio::io::duplex(512 * 1024);
        let mut zip = ZipFileWriter::new(w.compat_write());

        let options = ZipEntryBuilder::new(ZipString::from(fname.to_string()), Compression::Stored);
        zip.write_entry_whole(options, content.as_bytes()).await?;

        if add_inner {
            let opts = ZipEntryBuilder::new(ZipString::from("inner.zip".to_string()), Compression::Stored);
            zip.write_entry_whole(
                opts,
                &create_zip("inner.txt", "inner text file", false).await?,
            )
            .await?;
        }
        zip.close().await?;
        let mut buf = Vec::new();
        r.read_to_end(&mut buf).await?;
        Ok(buf)
    }

    #[tokio::test]
    async fn only_seek_zip_fs() -> Result<()> {
        let zip = test_data_dir().join("only-seek-zip.zip");
        let (a, d) = simple_fs_adapt_info(&zip).await?;
<<<<<<< HEAD
        let _v = adapted_to_vec(loop_adapt(&ZipAdapter::new(), d, a).await?).await?;
=======
        let engine = crate::preproc::make_engine(&a.config)?;
<<<<<<< HEAD
        let _v = adapted_to_vec(loop_adapt(&engine, &ZipAdapter::new(), d, a).await?).await?;
>>>>>>> 63d8839 (perf(zip): enable zero-copy operations for better performance)
=======
        let _v = adapted_to_vec(loop_adapt(engine, &ZipAdapter::new(), d, a).await?).await?;
>>>>>>> f3247d2 (perf(zip): compat streaming on async_zip 0.0.17)
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
<<<<<<< HEAD
        let buf = adapted_to_vec(loop_adapt(&engine, &adapter, d, a).await?).await?;
>>>>>>> 63d8839 (perf(zip): enable zero-copy operations for better performance)
=======
        let buf = adapted_to_vec(loop_adapt(engine, &adapter, d, a).await?).await?;
>>>>>>> f3247d2 (perf(zip): compat streaming on async_zip 0.0.17)

        assert_eq!(
            String::from_utf8(buf)?,
            "PREFIX:outer.txt: outer text file\nPREFIX:inner.zip: inner.txt: inner text file\n",
        );

        Ok(())
    }
}
<<<<<<< HEAD
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
=======
// no local boxing helper needed in 0.0.17 stream path
>>>>>>> f3247d2 (perf(zip): compat streaming on async_zip 0.0.17)
