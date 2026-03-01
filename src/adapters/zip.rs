use super::*;
use crate::print_bytes;
use anyhow::*;
use async_stream::stream;
use futures_lite::io::AsyncReadExt;
use lazy_static::lazy_static;
use log::*;
use tokio::io::{AsyncWriteExt, duplex};

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
        dbg!(&ai.filepath_hint, &ai.is_real_file);
        if ai.is_real_file {
            use async_zip::tokio::read::fs::ZipFileReader;
            use tokio::io::{copy, duplex};
            use tokio_util::compat::FuturesAsyncReadCompatExt;

            let AdaptInfo {
                filepath_hint,
                archive_recursion_depth,
                postprocess,
                line_prefix,
                config,
                ..
            } = ai;

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
        } else {
            return owned_zip_iter_fs(ai).await;
        }
    }
}

pub async fn owned_zip_iter_fs(ai: AdaptInfo) -> Result<AdaptedFilesIterBox> {
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

    let (zip_path, temp_file) = if is_real_file {
        (filepath_hint.clone(), None)
    } else {
        let tmp = tempfile::NamedTempFile::new()?;
        let tmp_path = tmp.path().to_path_buf();
        let mut f = tokio::fs::File::create(&tmp_path).await?;
        let mut r = inp;
        tokio::io::copy(&mut r, &mut f).await?;
        drop(f);
        (tmp_path, Some(tmp)) // keep temp file alive
    };

    let reader = ZipFileReader::new(&zip_path).await?;
    let metas: Vec<_> = reader.file().entries().to_vec();
    let s = stream! {
        let _temp_file_keeper = temp_file; // keep temp_file for duration of stream
        for (i, meta) in metas.into_iter().enumerate() {
            let name_str = String::from_utf8_lossy(meta.filename().as_bytes()).into_owned();
            dbg!(&name_str);
            if name_str.ends_with('/') { continue; }
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
    use async_zip::base::write::ZipFileWriter;
    use async_zip::{Compression, ZipEntryBuilder, ZipString};
    use tokio::io::AsyncReadExt;
    use tokio_util::compat::TokioAsyncWriteCompatExt;

    use super::*;
    use crate::{preproc::loop_adapt, test_utils::*};
    use pretty_assertions::assert_eq;

    #[async_recursion::async_recursion]
    async fn create_zip(fname: &str, content: &str, add_inner: bool) -> Result<Vec<u8>> {
        let (w, mut r) = tokio::io::duplex(512 * 1024);
        let mut zip = ZipFileWriter::new(w.compat_write());

        let options = ZipEntryBuilder::new(ZipString::from(fname.to_string()), Compression::Stored);
        zip.write_entry_whole(options, content.as_bytes()).await?;
        dbg!(fname);

        if add_inner {
            let opts = ZipEntryBuilder::new(
                ZipString::from("inner.zip".to_string()),
                Compression::Stored,
            );
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
        let engine = crate::preproc::make_engine(&a.config)?;
        let v = adapted_to_vec(loop_adapt(engine, &ZipAdapter::new(), d, a).await?).await?;

        let expected = vec![
            "PREFIX:META-INF/MANIFEST.MF: Manifest-Version: 1.0",
            "PREFIX:META-INF/MANIFEST.MF: Created-By: 1.3.0_02 (Sun Microsystems Inc.)",
            "PREFIX:META-INF/MANIFEST.MF: ",
            "PREFIX:META-INF/MANIFEST.MF: ",
            "PREFIX:layout/TableLayout$Entry.class: [rga: binary data]",
            "PREFIX:layout/TableLayout.class: [rga: binary data]",
            "PREFIX:layout/TableLayoutConstants.class: [rga: binary data]",
            "PREFIX:layout/TableLayoutConstraints.class: [rga: binary data]",
        ];
        let binding = String::from_utf8(v)?;
        let actual = binding.trim();

        // Compare line contents
        assert_eq!(expected, actual.lines().collect::<Vec<_>>());

        Ok(())
    }

    #[tokio::test]
    async fn only_seek_zip_mem() -> Result<()> {
        use tokio::fs::File;

        let zip = test_data_dir().join("only-seek-zip.zip");
        let (a, d) = simple_adapt_info(&zip, Box::pin(File::open(&zip).await?));
        let engine = crate::preproc::make_engine(&a.config)?;
        let v = adapted_to_vec(loop_adapt(engine, &ZipAdapter::new(), d, a).await?).await?;

        let expected = vec![
            "PREFIX:META-INF/MANIFEST.MF: Manifest-Version: 1.0",
            "PREFIX:META-INF/MANIFEST.MF: Created-By: 1.3.0_02 (Sun Microsystems Inc.)",
            "PREFIX:META-INF/MANIFEST.MF: ",
            "PREFIX:META-INF/MANIFEST.MF: ",
            "PREFIX:layout/TableLayout$Entry.class: [rga: binary data]",
            "PREFIX:layout/TableLayout.class: [rga: binary data]",
            "PREFIX:layout/TableLayoutConstants.class: [rga: binary data]",
            "PREFIX:layout/TableLayoutConstraints.class: [rga: binary data]",
        ];
        let binding = String::from_utf8(v)?;
        let actual = binding.trim();

        // Compare line contents
        assert_eq!(expected, actual.lines().collect::<Vec<_>>());

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
        let engine = crate::preproc::make_engine(&a.config)?;
        let buf = adapted_to_vec(loop_adapt(engine, &adapter, d, a).await?).await?;

        let expected = vec![
            "PREFIX:outer.txt: outer text file",
            "PREFIX:inner.zip: inner.txt: inner text file",
        ];
        let binding = String::from_utf8(buf)?;
        let actual = binding.trim();

        // Compare line contents
        assert_eq!(expected, actual.lines().collect::<Vec<_>>());

        Ok(())
    }
}
