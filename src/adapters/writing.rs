use std::pin::Pin;

use crate::{adapted_iter::one_file, join_handle_to_stream, to_io_err};

use super::{AdaptInfo, Adapter, FileAdapter};
use anyhow::{Context, Result};
use async_trait::async_trait;
use tokio::io::{AsyncReadExt, AsyncWrite};

#[async_trait]
pub trait WritingFileAdapter: Adapter + Send + Sync + Clone {
    async fn adapt_write(
        a: super::AdaptInfo,
        detection_reason: &crate::matching::FileMatcher,
        oup: Pin<Box<dyn AsyncWrite + Send>>,
    ) -> Result<()>;
}

macro_rules! async_writeln {
    ($dst: expr) => {
        {
            tokio::io::AsyncWriteExt::write_all(&mut $dst, b"\n").await
        }
    };
    ($dst: expr, $fmt: expr) => {
        {
            use std::io::Write;
            let mut buf = Vec::<u8>::new();
            writeln!(buf, $fmt)?;
            tokio::io::AsyncWriteExt::write_all(&mut $dst, &buf).await
        }
    };
    ($dst: expr, $fmt: expr, $($arg: tt)*) => {
        {
            use std::io::Write;
            let mut buf = Vec::<u8>::new();
            writeln!(buf, $fmt, $( $arg )*)?;
            tokio::io::AsyncWriteExt::write_all(&mut $dst, &buf).await
        }
    };
}
pub(crate) use async_writeln;

#[async_trait]
impl<T> FileAdapter for T
where
    T: WritingFileAdapter,
{
    async fn adapt(
        &self,
        a: super::AdaptInfo,
        detection_reason: &crate::matching::FileMatcher,
    ) -> Result<crate::adapted_iter::AdaptedFilesIterBox> {
        let name = self.metadata().name.clone();
        let (w, r) = tokio::io::duplex(128 * 1024);
        let d2 = detection_reason.clone();
        let archive_recursion_depth = a.archive_recursion_depth + 1;
        let filepath_hint = format!("{}.txt", a.filepath_hint.to_string_lossy());
        let postprocess = a.postprocess;
        let line_prefix = a.line_prefix.clone();
        let config = a.config.clone();
        let joiner = tokio::spawn(async move {
            let x = d2;
            T::adapt_write(a, &x, Box::pin(w))
                .await
                .with_context(|| format!("in {}.adapt_write", name))
                .map_err(to_io_err)
        });

        Ok(one_file(AdaptInfo {
            is_real_file: false,
            filepath_hint: filepath_hint.into(),
            archive_recursion_depth,
            config,
            inp: Box::pin(r.chain(join_handle_to_stream(joiner))),
            line_prefix,
            postprocess,
        }))
    }
}
