use super::*;
use crate::print_bytes;
use anyhow::*;
use async_stream::stream;
use lazy_static::lazy_static;
use log::*;
use tokio::io::AsyncReadExt;

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

fn make_zip_adapt_info(
    filename: String,
    buf: Vec<u8>,
    line_prefix: &str,
    archive_recursion_depth: i32,
    postprocess: bool,
    config: &RgaConfig,
) -> AdaptInfo {
    let new_line_prefix = format!("{}{}: ", line_prefix, filename);
    let s = async_stream::stream! { yield std::io::Result::Ok(bytes::Bytes::from(buf)); };
    AdaptInfo {
        filepath_hint: PathBuf::from(filename),
        is_real_file: false,
        file_mtime_unix_ms: None,
        inp: Box::pin(tokio_util::io::StreamReader::new(s)),
        line_prefix: new_line_prefix,
        archive_recursion_depth: archive_recursion_depth + 1,
        postprocess,
        config: config.clone(),
    }
}

#[async_trait]
impl FileAdapter for ZipAdapter {
    async fn adapt(
        &self,
        ai: AdaptInfo,
        _detection_reason: &FileMatcher,
    ) -> Result<AdaptedFilesIterBox> {
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
                    tokio::pin!(reader);
                    let mut buf = Vec::new();
                    if file.uncompressed_size() < 10_000_000 {
                        buf.reserve(file.uncompressed_size() as usize);
                    }
                    reader.read_to_end(&mut buf).await?;
                    yield Ok(make_zip_adapt_info(
                        file.filename().to_string(),
                        buf,
                        &line_prefix,
                        archive_recursion_depth,
                        postprocess,
                        &config,
                    ));
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
                    let filename = file.filename().to_string();
                    let uncompressed = file.uncompressed_size();
                    let compressed = file.compressed_size();
                    if filename.ends_with('/') {
                        zip = entry.skip().await?;
                        continue;
                    }
                    debug!(
                        "{}{}|{}: {} ({} packed)",
                        line_prefix,
                        filepath_hint.display(),
                        filename,
                        print_bytes(uncompressed as f64),
                        print_bytes(compressed as f64)
                    );
                    let reader = entry.reader();
                    tokio::pin!(reader);
                    let mut buf = Vec::new();
                    if uncompressed < 10_000_000 {
                        buf.reserve(uncompressed as usize);
                    }
                    reader.read_to_end(&mut buf).await?;
                    yield Ok(make_zip_adapt_info(
                        filename,
                        buf,
                        &line_prefix,
                        archive_recursion_depth,
                        postprocess,
                        &config,
                    ));
                    zip = entry.done().await.context("going to next file in zip but entry was not read fully")?;
                }
                trace!("zip over");
            };

            Ok(Box::pin(s))
        }
    }
}

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
        let _v = adapted_to_vec(loop_adapt(&ZipAdapter::new(), d, a, crate::adapters::get_all_adapters(None).0).await?).await?;
        Ok(())
    }

    #[tokio::test]
    async fn recurse() -> Result<()> {
        let zipfile = create_zip("outer.txt", "outer text file", true).await?;
        let adapter = ZipAdapter::new();

        let (a, d) = simple_adapt_info(
            &PathBuf::from("outer.zip"),
            Box::pin(std::io::Cursor::new(zipfile)),
        );
        let buf = adapted_to_vec(loop_adapt(&adapter, d, a, crate::adapters::get_all_adapters(None).0).await?).await?;

        assert_eq!(
            String::from_utf8(buf)?,
            "PREFIX:outer.txt: outer text file\nPREFIX:inner.zip: inner.txt: inner text file\n",
        );

        Ok(())
    }
}
