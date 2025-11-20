use super::*;
use crate::print_bytes;
use anyhow::*;
use async_stream::stream;
use tokio::io::duplex;
use futures_lite::io::AsyncReadExt;
use tokio::sync::Semaphore;
use tokio::sync::{mpsc, Mutex};
use std::time::Instant;
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
        keep_fast_matchers_if_accurate: true,
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
        if ai.config.zip_owned_iter {
            return owned_zip_iter_fs(ai).await;
        }
        if !ai.is_real_file {
            return owned_zip_iter_fs(ai).await;
        }
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
            let sem = std::sync::Arc::new(Semaphore::new(config.zip_max_concurrency.0));
            let held = std::sync::Arc::new(Mutex::new(Vec::<tokio::sync::OwnedSemaphorePermit>::new()));
            let (tx, mut rx) = mpsc::unbounded_channel::<(usize, f64)>();
            {
                let sem2 = sem.clone();
                let held2 = held.clone();
                let maxc = config.zip_max_concurrency.0;
                tokio::spawn(async move {
                    let mut samples: Vec<(usize, f64)> = Vec::new();
                    let mut target = maxc;
                    while let Some(s) = rx.recv().await {
                        samples.push(s);
                        if samples.len() >= 8 {
                            let mut sum = 0.0f64;
                            for (_, secs) in samples.iter() { sum += *secs; }
                            let avg = sum / (samples.len() as f64);
                            if avg > 0.2 && target > 2 {
                                target -= 1;
                                let mut h = held2.lock().await;
                                let p = sem2.clone().acquire_owned().await.unwrap();
                                h.push(p);
                            } else if avg < 0.05 && target < maxc {
                                target += 1;
                                let mut h = held2.lock().await;
                                if let Some(p) = h.pop() { drop(p); }
                            }
                            samples.clear();
                        }
                    }
                });
            }
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
                    let (w, r) = duplex(config.zip_pipe_bytes.0);
                    let path_for_task = filepath_hint.clone();
                    let tx2 = tx.clone();
                    let sem_c = sem.clone();
                    tokio::spawn(async move {
                        let _permit = sem_c.acquire_owned().await.unwrap();
                        let zip2 = ZipFileReader::new(&path_for_task).await?;
                        let reader_with_entry = zip2.reader_with_entry(i).await?;
                        let fut_reader = reader_with_entry.boxed_reader();
                        let mut src = FuturesAsyncReadCompatExt::compat(fut_reader);
                        let mut dst = w;
                        let start = Instant::now();
                        let n = copy(&mut src, &mut dst).await?;
                        let secs = (Instant::now() - start).as_secs_f64();
                        let _ = tx2.send((n as usize, secs));
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

#[allow(dead_code)]
pub async fn owned_zip_iter_fs(
    ai: AdaptInfo,
) -> Result<AdaptedFilesIterBox> {
    use async_zip::tokio::read::fs::ZipFileReader;
    // no compat needed in buffered path
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
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let tmp_path = std::env::temp_dir().join(format!("rga_zip_{}.zip", ts));
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
            let (mut w, r) = duplex(config.zip_pipe_bytes.0);
            let data = buf;
            use tokio::io::AsyncWriteExt;
            tokio::spawn(async move { let _ = w.write_all(&data).await; });
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
