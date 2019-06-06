use super::*;
use failure::*;
use std::io::prelude::*;
use std::io::BufReader;
use std::process::Command;
use std::process::Stdio;


pub fn postproc_line_prefix(
    line_prefix: &str,
    inp: &mut dyn Read,
    oup: &mut dyn Write,
) -> Fallible<()> {
    //std::io::copy(inp, oup)?;

    for line in BufReader::new(inp).lines() {
        oup.write_all(format!("{}{}\n", line_prefix, line?).as_bytes())?;
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

pub fn map_exe_error(err: std::io::Error, exe_name: &str, help: &str) -> Error {
    use std::io::ErrorKind::*;
    match err.kind() {
        NotFound => format_err!("Could not find executable \"{}\". {}", exe_name, help),
        _ => Error::from(err),
    }
}

/*fn pipe(a: &mut dyn Read, b: &mut dyn Write, c: &mut dyn Read, d: &mut dyn Write) {
    let mut buf = vec![0u8; 2 << 13];
    loop {
        match a.read(&buf) {

        }
    }
}*/

/*pub fn copy<R: ?Sized, W: ?Sized>(
    name: &str,
    reader: &mut R,
    writer: &mut W,
) -> std::io::Result<u64>
where
    R: Read,
    W: Write,
{
    eprintln!("START COPY {}", name);
    let mut zz = vec![0; 1 << 13];
    let mut buf: &mut [u8] = zz.as_mut();
    let mut written = 0;
    loop {
        let r = reader.read(buf);
        eprintln!("{}read: {:?}", name, r);
        let len = match r {
            Ok(0) => return Ok(written),
            Ok(len) => len,
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        writer.write_all(&buf[..len])?;
        written += len as u64;
    }
}*/

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
    fn adapt(&self, ai: AdaptInfo) -> Fallible<()> {
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
