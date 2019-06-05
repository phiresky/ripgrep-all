use rga::adapters::*;
use std::process::Command;

fn main() -> std::io::Result<()> {
    let adapters = get_adapters();

    let extensions = adapters
        .iter()
        .flat_map(|a| &a.metadata().matchers)
        .filter_map(|m| match m {
            Matcher::FileExtension(ext) => Some(ext as &str),
        })
        .collect::<Vec<_>>()
        .join(",");

    let exe = std::env::current_exe().expect("Could not get executable location");
    let preproc_exe = exe.with_file_name("rga-preproc");
    let mut child = Command::new("rg")
        .arg("--no-line-number")
        .arg("--pre")
        .arg(preproc_exe)
        .arg("--pre-glob")
        .arg(format!("*.{{{}}}", extensions))
        .args(std::env::args().skip(1))
        .spawn()?;

    child.wait()?;
    Ok(())
}
