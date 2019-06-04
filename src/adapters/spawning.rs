use super::*;
use std::io::Write;
use std::process::Command;
use std::process::Stdio;

pub trait SpawningFileAdapter: GetMetadata {
    fn command(&self, inp_fname: &str) -> Command;
}

impl<T> FileAdapter for T
where
    T: SpawningFileAdapter,
{
    fn adapt(&self, inp_fname: &str, oup: &mut dyn Write) -> std::io::Result<()> {
        let mut cmd = self.command(inp_fname).stdout(Stdio::piped()).spawn()?;
        let stdo = cmd.stdout.as_mut().expect("is piped");
        std::io::copy(stdo, oup)?;
        let status = cmd.wait()?;
        if status.success() {
            Ok(())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "subprocess failed",
            ))
        }
    }
}
