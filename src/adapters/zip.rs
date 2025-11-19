use super::*;
use crate::print_bytes;
use anyhow::*;
use async_stream::stream;
use futures_lite::io::AsyncReadExt;
//
use lazy_static::lazy_static;
use log::*;
// tokio AsyncRead not needed in 0.0.17 stream path with compat boxing
// fs reader path for async_zip 0.0.12

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
                        line_prefix: new_line_prefix,
                        archive_recursion_depth: archive_recursion_depth + 1,
                        postprocess,
                        config: config.clone(),
                    });
                }
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
        let engine = crate::preproc::make_engine(&a.config)?;
        let _v = adapted_to_vec(loop_adapt(engine, &ZipAdapter::new(), d, a).await?).await?;
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
        let engine = crate::preproc::make_engine(&a.config)?;
        let buf = adapted_to_vec(loop_adapt(engine, &adapter, d, a).await?).await?;

        assert_eq!(
            String::from_utf8(buf)?,
            "PREFIX:outer.txt: outer text file\nPREFIX:inner.zip: inner.txt: inner text file\n",
        );

        Ok(())
    }
}
// no local boxing helper needed in 0.0.17 stream path
