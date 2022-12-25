use super::*;
use anyhow::Result;
use async_stream::{stream, AsyncStream};
use bytes::{Buf, Bytes};
use log::*;
use tokio_util::io::StreamReader;

use crate::adapters::FileAdapter;
use crate::expand::expand_str_ez;
use std::future::Future;
use std::path::Path;
use std::process::{ExitStatus, Stdio};
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};

// TODO: don't separate the trait and the struct
pub trait SpawningFileAdapterTrait: GetMetadata + Send + Sync {
    fn get_exe(&self) -> &str;
    fn command(&self, filepath_hint: &Path, command: Command) -> Result<Command>;
}

pub struct SpawningFileAdapter {
    inner: Box<dyn SpawningFileAdapterTrait>,
}

impl SpawningFileAdapter {
    pub fn new(inner: Box<dyn SpawningFileAdapterTrait>) -> SpawningFileAdapter {
        SpawningFileAdapter { inner }
    }
}

impl GetMetadata for SpawningFileAdapter {
    fn metadata(&self) -> &AdapterMeta {
        self.inner.metadata()
    }
}


/// replace a Command.spawn() error "File not found" with a more readable error
/// to indicate some program is not installed
pub fn map_exe_error(err: std::io::Error, exe_name: &str, help: &str) -> Error {
    use std::io::ErrorKind::*;
    match err.kind() {
        NotFound => format_err!("Could not find executable \"{}\". {}", exe_name, help),
        _ => Error::from(err),
    }
}

/** waits for a process to finish, returns an io error if the process failed */
struct ProcWaitReader {
    process: Option<Child>,
    future: Option<Pin<Box<dyn Future<Output = std::io::Result<ExitStatus>>>>>,
}
impl ProcWaitReader {
    fn new(cmd: Child) -> ProcWaitReader {
        ProcWaitReader {
            process: Some(cmd),
            future: None,
        }
    }
}
fn proc_wait(mut child: Child) -> impl AsyncRead {
    let s = stream! {
        let res = child.wait().await?;
        if res.success() {
            yield std::io::Result::Ok(Bytes::new());
        } else {
            yield std::io::Result::Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format_err!("subprocess failed: {:?}", res),
            ));
        }
    };
    StreamReader::new(s)
}
pub fn pipe_output<'a>(
    _line_prefix: &str,
    mut cmd: Command,
    inp: ReadBox,
    exe_name: &str,
    help: &str,
) -> Result<ReadBox> {
    let mut cmd = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| map_exe_error(e, exe_name, help))?;
    let mut stdi = cmd.stdin.take().expect("is piped");
    let stdo = cmd.stdout.take().expect("is piped");

    tokio::spawn(async move {
        let mut z = inp;
        tokio::io::copy(&mut z, &mut stdi).await.unwrap();
    });
    Ok(Box::pin(stdo.chain(proc_wait(cmd))))
}

impl FileAdapter for SpawningFileAdapter {
    fn adapt<'a>(
        &self,
        ai: AdaptInfo,
        _detection_reason: &FileMatcher,
    ) -> Result<AdaptedFilesIterBox> {
        let AdaptInfo {
            filepath_hint,
            inp,
            line_prefix,
            archive_recursion_depth,
            postprocess,
            config,
            ..
        } = ai;

        let cmd = Command::new(self.inner.get_exe());
        let cmd = self
            .inner
            .command(&filepath_hint, cmd)
            .with_context(|| format!("Could not set cmd arguments for {}", self.inner.get_exe()))?;
        debug!("executing {:?}", cmd);
        let output = pipe_output(&line_prefix, cmd, inp, self.inner.get_exe(), "")?;
        Ok(Box::pin(tokio_stream::once(AdaptInfo {
            filepath_hint: PathBuf::from(
                expand_str_ez(self.inner.output_path_hint, |r| match r {
                    "fullname" => &filepath_hint.to_string_lossy()
                }
            )),
            inp: output,
            line_prefix,
            is_real_file: false,
            archive_recursion_depth,
            postprocess,
            config,
        })))
    }
}

#[cfg(test)]
mod test {
    use std::io::Cursor;

    use super::*;
    use crate::adapters::FileAdapter;
    use crate::{
        adapters::custom::CustomAdapterConfig,
        test_utils::{adapted_to_vec, simple_adapt_info},
    };

    #[tokio::test]
    async fn streaming() -> anyhow::Result<()> {
        // an adapter that converts input line by line (deadlocks if the parent process tries to write everything and only then read it)
        let adapter = CustomAdapterConfig {
            name: "simple text replacer".to_string(),
            description: "oo".to_string(),
            disabled_by_default: None,
            version: 1,
            extensions: vec!["txt".to_string()],
            mimetypes: None,
            match_only_by_mime: None,
            binary: "sed".to_string(),
            args: vec!["s/e/u/g".to_string()],
        };

        let adapter = adapter.to_adapter();
        let input = r#"
        This is the story of a
        very strange lorry
        with a long dead crew
        and a witch with the flu
        "#;
        let input = format!("{0}{0}{0}{0}", input);
        let input = format!("{0}{0}{0}{0}", input);
        let input = format!("{0}{0}{0}{0}", input);
        let input = format!("{0}{0}{0}{0}", input);
        let input = format!("{0}{0}{0}{0}", input);
        let input = format!("{0}{0}{0}{0}", input);
        let (a, d) = simple_adapt_info(
            &Path::new("foo.txt"),
            Box::pin(Cursor::new(Vec::from(input))),
        );
        let output = adapter.adapt(a, &d).unwrap();

        let oup = adapted_to_vec(output).await?;
        println!("output: {}", String::from_utf8_lossy(&oup));
        Ok(())
    }
}
