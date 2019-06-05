use super::*;
use std::io::Write;
use std::process::Command;
use std::process::Stdio;
use failure::*;

pub trait SpawningFileAdapter: GetMetadata {
    fn get_exe(&self) -> &str;
    fn command(&self, inp_fname: &Path, command: Command) -> Command;
}

pub fn map_exe_error(err: std::io::Error, exe_name: &str, help: &str) -> Error {
    use std::io::ErrorKind::*;
    match err.kind() {
        NotFound => format_err!("Could not find executable \"{}\". {}", exe_name, help),
        _ => Error::from(err)
    }
}

pub fn pipe_output(mut cmd: Command, oup: &mut dyn Write, exe_name: &str, help: &str) -> Fallible<()> {
    let mut cmd = cmd.stdout(Stdio::piped()).spawn().map_err(|e| map_exe_error(e, exe_name, help))?;
    let stdo = cmd.stdout.as_mut().expect("is piped");
    std::io::copy(stdo, oup)?;
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
    fn adapt(&self, inp_fname: &Path, oup: &mut dyn Write) -> Fallible<()> {
        let cmd = Command::new(self.get_exe());
        pipe_output(self.command(inp_fname, cmd), oup, self.get_exe(), "")
    }
}
