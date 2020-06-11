use super::*;
use anyhow::*;
use encoding_rs_io::DecodeReaderBytesBuilder;
use log::*;
use regex::Regex;
use std::io::prelude::*;
use std::io::BufReader;
use std::process::Command;
use std::process::{Child, Stdio};

/**
 * Copy a Read to a Write, while prefixing every line with a prefix.
 *
 * Try to detect binary files and ignore them. Does not ensure any encoding in the output.
 *
 * Binary detection is needed because the rg binary detection does not apply to preprocessed files
 */

/**/
pub fn postproc_line_prefix(
    line_prefix: &str,
    inp: &mut dyn Read,
    oup: &mut dyn Write,
) -> Result<()> {
    // TODO: parse these options from ripgrep's configuration
    let encoding = None; // detect bom but usually assume utf8
    let bom_sniffing = true;
    let mut decode_builder = DecodeReaderBytesBuilder::new();
    // https://github.com/BurntSushi/ripgrep/blob/a7d26c8f144a4957b75f71087a66692d0b25759a/grep-searcher/src/searcher/mod.rs#L706
    let inp = decode_builder
        .encoding(encoding)
        .utf8_passthru(true)
        .strip_bom(bom_sniffing)
        .bom_override(true)
        .bom_sniffing(bom_sniffing)
        .build(inp);
    // check for null byte in first 8kB
    let mut reader = BufReader::with_capacity(1 << 12, inp);
    let fourk = reader.fill_buf()?;
    if fourk.contains(&0u8) {
        writeln!(oup, "{}[rga: binary data]\n", line_prefix)?;
        return Ok(());
    }
    // intentionally do not call reader.consume
    for line in reader.split(b'\n') {
        let line = line?;
        if line.contains(&0u8) {
            writeln!(oup, "{}[rga: binary data]\n", line_prefix)?;
            return Ok(());
        }
        oup.write_all(line_prefix.as_bytes())?;
        oup.write_all(&line)?;
        oup.write_all(b"\n")?;
    }
    Ok(())
}
pub trait SpawningFileAdapterTrait: GetMetadata {
    fn get_exe(&self) -> &str;
    fn command(&self, filepath_hint: &Path, command: Command) -> Result<Command>;

    /*fn postproc(&self, line_prefix: &str, inp: &mut dyn Read, oup: &mut dyn Write) -> Result<()> {
        postproc_line_prefix(line_prefix, inp, oup)
    }*/
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

/*impl<T: SpawningFileAdapterTrait> From<T> for SpawningFileAdapter {
    fn from(e: dyn T) -> Self {
        SpawningFileAdapter { inner: Box::new(e) }
    }
}*/

/// replace a Command.spawn() error "File not found" with a more readable error
/// to indicate some program is not installed
pub fn map_exe_error(err: std::io::Error, exe_name: &str, help: &str) -> Error {
    use std::io::ErrorKind::*;
    match err.kind() {
        NotFound => format_err!("Could not find executable \"{}\". {}", exe_name, help),
        _ => Error::from(err),
    }
}

struct ProcWaitReader {
    proce: Child,
}
impl Read for ProcWaitReader {
    fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
        let status = self.proce.wait()?;
        if status.success() {
            Ok(0)
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format_err!("subprocess failed: {:?}", status),
            ))
        }
    }
}
pub fn pipe_output(
    _line_prefix: &str,
    mut cmd: Command,
    inp: &mut (dyn Read),
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

    // TODO: how to handle this copying better?
    // do we really need threads for this?
    crossbeam::scope(|_s| -> Result<()> {
        std::io::copy(inp, &mut stdi)?;
        drop(stdi); // NEEDED! otherwise deadlock
        Ok(())
    })
    .unwrap()?;
    Ok(Box::new(stdo.chain(ProcWaitReader { proce: cmd })))
}

impl FileAdapter for SpawningFileAdapter {
    fn adapt(&self, ai: AdaptInfo, _detection_reason: &SlowMatcher) -> Result<ReadBox> {
        let AdaptInfo {
            filepath_hint,
            mut inp,
            line_prefix,
            ..
        } = ai;

        let cmd = Command::new(self.inner.get_exe());
        let cmd = self
            .inner
            .command(&filepath_hint, cmd)
            .with_context(|| format!("Could not set cmd arguments for {}", self.inner.get_exe()))?;
        debug!("executing {:?}", cmd);
        pipe_output(&line_prefix, cmd, &mut inp, self.inner.get_exe(), "")
    }
}
