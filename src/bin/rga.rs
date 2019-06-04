use rga::adapters;

use std::process::Command;

fn main() -> std::io::Result<()> {
    let exe = std::env::current_exe().expect("Could not get executable location");
    let preproc_exe = exe.with_file_name("rga-preproc");
    let mut child = Command::new("rg")
        .arg("--pre")
        .arg(preproc_exe)
        .args(std::env::args().skip(1))
        .spawn()?;

    child.wait()?;
    Ok(())
}
