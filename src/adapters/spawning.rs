use super::*;
use encoding_rs_io::DecodeReaderBytesBuilder;
use failure::*;
use std::io::prelude::*;
use std::io::BufReader;
use std::process::Command;
use std::process::Stdio;

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
) -> Fallible<()> {
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
pub trait SpawningFileAdapter: GetMetadata {
    fn get_exe(&self) -> &str;
    fn command(&self, filepath_hint: &Path, command: Command) -> Command;

    fn postproc(line_prefix: &str, inp: &mut dyn Read, oup: &mut dyn Write) -> Fallible<()> {
        postproc_line_prefix(line_prefix, inp, oup)
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
pub fn pipe_output(
    line_prefix: &str,
    mut cmd: Command,
    inp: &mut (dyn Read),
    oup: &mut (dyn Write + Send),
    exe_name: &str,
    help: &str,
    cp: fn(line_prefix: &str, &mut dyn Read, &mut dyn Write) -> Fallible<()>,
) -> Fallible<()> {
    let mut cmd = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| map_exe_error(e, exe_name, help))?;
    let mut stdi = cmd.stdin.take().expect("is piped");
    let mut stdo = cmd.stdout.take().expect("is piped");

    // TODO: how to handle this copying better?
    // do we really need threads for this?
    crossbeam::scope(|s| -> Fallible<()> {
        s.spawn(|_| cp(line_prefix, &mut stdo, oup).unwrap()); // errors?
        std::io::copy(inp, &mut stdi)?;
        drop(stdi); // NEEDED! otherwise deadlock
        Ok(())
    })
    .unwrap()?;
    let status = cmd.wait()?;
    if status.success() {
        Ok(())
    } else {
        Err(format_err!("subprocess failed: {:?}", status))
    }
}

impl<T> FileAdapter for T
where
    T: SpawningFileAdapter,
{
    fn adapt(&self, ai: AdaptInfo, _detection_reason: &SlowMatcher) -> Fallible<()> {
        let AdaptInfo {
            filepath_hint,
            mut inp,
            oup,
            line_prefix,
            ..
        } = ai;
        let cmd = Command::new(self.get_exe());
        pipe_output(
            line_prefix,
            self.command(filepath_hint, cmd),
            &mut inp,
            oup,
            self.get_exe(),
            "",
            Self::postproc,
        )
    }
}
