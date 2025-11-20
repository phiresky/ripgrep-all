use crate::adapted_iter::one_file;

use super::*;

use anyhow::Result;
use lazy_static::lazy_static;
use tokio::io::BufReader;
use std::time::Instant;
use tokio::io::{AsyncRead, ReadBuf};
use std::pin::Pin;
use std::task::{Context as TaskContext, Poll};

use std::path::{Path, PathBuf};

static EXTENSIONS: &[&str] = &["als", "bz2", "gz", "tbz", "tbz2", "tgz", "xz", "zst"];
static MIME_TYPES: &[&str] = &[
    "application/gzip",
    "application/x-bzip",
    "application/x-xz",
    "application/zstd",
];
lazy_static! {
    static ref METADATA: AdapterMeta = AdapterMeta {
        name: "decompress".to_owned(),
        version: 1,
        description:
            "Reads compressed file as a stream and runs a different extractor on the contents."
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
        disabled_by_default: false,
        keep_fast_matchers_if_accurate: true
    };
}
#[derive(Default)]
pub struct DecompressAdapter;

impl DecompressAdapter {
    pub fn new() -> Self {
        Self
    }
}
impl GetMetadata for DecompressAdapter {
    fn metadata(&self) -> &AdapterMeta {
        &METADATA
    }
}

fn decompress_any(reason: &FileMatcher, inp: ReadBox, cfg: &crate::config::RgaConfig) -> Result<ReadBox> {
    use FastFileMatcher::*;
    use FileMatcher::*;
    use async_compression::tokio::bufread;
    let mk = |alg: &'static str, inp: ReadBox| -> ReadBox {
        let cap = if cfg.decompress_autotune { cap_for(alg, cfg) } else {
            match alg { "gzip" => cfg.decompress_gzip_buf_bytes.0, "bzip2" => cfg.decompress_bzip2_buf_bytes.0, "xz" => cfg.decompress_xz_buf_bytes.0, "zstd" => cfg.decompress_zstd_buf_bytes.0, _ => 1 << 16 }
        };
        let rd = BufReader::with_capacity(cap, inp);
        let r: ReadBox = match alg {
            "gzip" => Box::pin(bufread::GzipDecoder::new(rd)),
            "bzip2" => Box::pin(bufread::BzDecoder::new(rd)),
            "xz" => Box::pin(bufread::XzDecoder::new(rd)),
            _ => Box::pin(bufread::ZstdDecoder::new(rd)),
        };
        if cfg.decompress_autotune { MeasuredBox::wrap(r, alg) } else { r }
    };

    Ok(match reason {
        Fast(FileExtension(ext)) => match ext.as_ref() {
            "als" | "gz" | "tgz" => mk("gzip", inp),
            "bz2" | "tbz" | "tbz2" => mk("bzip2", inp),
            "zst" => mk("zstd", inp),
            "xz" => mk("xz", inp),
            ext => Err(format_err!("don't know how to decompress {}", ext))?,
        },
        MimeType(mime) => match mime.as_ref() {
            "application/gzip" => mk("gzip", inp),
            "application/x-bzip" => mk("bzip2", inp),
            "application/x-xz" => mk("xz", inp),
            "application/zstd" => mk("zstd", inp),
            mime => Err(format_err!("don't know how to decompress mime {}", mime))?,
        },
    })
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

#[async_trait]
impl FileAdapter for DecompressAdapter {
    async fn adapt(
        &self,
        ai: AdaptInfo,
        detection_reason: &FileMatcher,
    ) -> Result<AdaptedFilesIterBox> {
        Ok(one_file(AdaptInfo {
            filepath_hint: get_inner_filename(&ai.filepath_hint),
            is_real_file: false,
            archive_recursion_depth: ai.archive_recursion_depth + 1,
            inp: decompress_any(detection_reason, ai.inp, &ai.config)?,
            line_prefix: ai.line_prefix,
            config: ai.config.clone(),
            postprocess: ai.postprocess,
        }))
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
        let adapter = DecompressAdapter;

        let filepath = test_data_dir().join("hello.gz");

        let (a, d) = simple_adapt_info(&filepath, Box::pin(File::open(&filepath).await?));
        let r = adapter.adapt(a, &d).await?;
        let o = adapted_to_vec(r).await?;
        assert_eq!(String::from_utf8(o)?, "hello\n");
        Ok(())
    }

    #[tokio::test]
    async fn pdf_gz() -> Result<()> {
        let adapter = DecompressAdapter;

        let filepath = test_data_dir().join("short.pdf.gz");

        let (a, d) = simple_adapt_info(&filepath, Box::pin(File::open(&filepath).await?));
        let engine = crate::preproc::make_engine(&a.config)?;
        let r = loop_adapt(engine, &adapter, d, a).await?;
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
lazy_static! {
    static ref DECOMP_CAPS: std::sync::Mutex<Option<(usize, usize, usize, usize)>> = std::sync::Mutex::new(None);
}
pub fn tuned_caps() -> Option<(usize, usize, usize, usize)> { *DECOMP_CAPS.lock().unwrap() }
pub fn set_caps_all(gz: usize, bz2: usize, xz: usize, z: usize) { let mut g = DECOMP_CAPS.lock().unwrap(); *g = Some((gz,bz2,xz,z)); }
pub fn import_caps_from_file(path: &str) -> anyhow::Result<()> {
    let s = std::fs::read_to_string(path)?;
    let v: serde_json::Value = serde_json::from_str(&s)?;
    let gz = v.get("gzip").and_then(|x| x.as_u64()).unwrap_or(65536) as usize;
    let bz2 = v.get("bzip2").and_then(|x| x.as_u64()).unwrap_or(65536) as usize;
    let xz = v.get("xz").and_then(|x| x.as_u64()).unwrap_or(131072) as usize;
    let z = v.get("zstd").and_then(|x| x.as_u64()).unwrap_or(524288) as usize;
    set_caps_all(gz,bz2,xz,z);
    Ok(())
}
pub fn export_caps_to_file(path: &str) -> anyhow::Result<()> {
    if let Some((gz,bz2,xz,z)) = tuned_caps() {
        let v = serde_json::json!({"gzip":gz, "bzip2":bz2, "xz":xz, "zstd":z});
        std::fs::write(path, serde_json::to_string(&v)?)?;
    }
    Ok(())
}
fn get_caps(cfg: &crate::config::RgaConfig) -> (usize, usize, usize, usize) {
    let mut g = DECOMP_CAPS.lock().unwrap();
    if let Some(c) = *g { c } else {
        let c = (
            cfg.decompress_gzip_buf_bytes.0,
            cfg.decompress_bzip2_buf_bytes.0,
            cfg.decompress_xz_buf_bytes.0,
            cfg.decompress_zstd_buf_bytes.0,
        );
        *g = Some(c);
        c
    }
}
fn cap_current_for(alg: &str) -> Option<usize> {
    if let Some((gz,bz2,xz,z)) = tuned_caps() {
        Some(match alg { "gzip" => gz, "bzip2" => bz2, "xz" => xz, "zstd" => z, _ => 1<<16 })
    } else { None }
}
fn cap_for(alg: &str, cfg: &crate::config::RgaConfig) -> usize {
    let (gz, bz2, xz, z) = get_caps(cfg);
    match alg { "gzip" => gz, "bzip2" => bz2, "xz" => xz, "zstd" => z, _ => 1 << 16 }
}
fn set_caps(alg: &str, new_cap: usize) {
    let mut g = DECOMP_CAPS.lock().unwrap();
    if let Some(ref mut t) = *g {
        match alg { "gzip" => t.0 = new_cap, "bzip2" => t.1 = new_cap, "xz" => t.2 = new_cap, "zstd" => t.3 = new_cap, _ => {} }
    }
}
struct MeasuredBox {
    inner: super::ReadBox,
    start: Instant,
    bytes: u64,
    alg: &'static str,
    updated: bool,
}
impl MeasuredBox {
    fn wrap(inner: super::ReadBox, alg: &'static str) -> super::ReadBox {
        Box::pin(MeasuredBox { inner, start: Instant::now(), bytes: 0, alg, updated: false })
    }
}
impl AsyncRead for MeasuredBox {
    fn poll_read(self: Pin<&mut Self>, cx: &mut TaskContext<'_>, buf: &mut ReadBuf<'_>) -> Poll<std::io::Result<()>> {
        let me = unsafe { self.get_unchecked_mut() };
        let before = buf.filled().len();
        let res = Pin::new(&mut me.inner).poll_read(cx, buf);
        if let Poll::Ready(Ok(())) = res {
            let after = buf.filled().len();
            me.bytes += (after.saturating_sub(before)) as u64;
            if after == before && !me.updated {
                let secs = me.start.elapsed().as_secs_f64();
                if secs > 0.0 {
                    let mbps = (me.bytes as f64) / secs / (1024.0 * 1024.0);
                    if let Some(cur) = cap_current_for(me.alg) {
                        let (min_cap, max_cap) = match me.alg { "gzip" => (1 << 16, 1 << 20), "bzip2" => (1 << 16, 1 << 19), "xz" => (1 << 16, 1 << 19), "zstd" => (1 << 16, 1 << 20), _ => (1 << 16, 1 << 20) };
                        let mut new = cur;
                        if mbps < 80.0 { new = (cur.saturating_mul(2)).min(max_cap); }
                        else if mbps > 250.0 { new = (cur.saturating_div(2)).max(min_cap); }
                        if new != cur { set_caps(me.alg, new); }
                    }
                }
                me.updated = true;
            }
        }
        res
    }
}
