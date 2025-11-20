use anyhow::Context;

use std::process::Command;

// TODO: add --rg-params=..., --rg-preview-params=... and --fzf-params=... params
// TODO: remove passthrough_args
fn main() -> anyhow::Result<()> {
    env_logger::init();
    let mut args = std::env::args().skip(1);
    let query = args.next().context("no query")?;
    let mut files: Vec<String> = Vec::new();
    for f in args { files.push(f); }
    if files.is_empty() { anyhow::bail!("no filename"); }
    // let instance_id = std::env::var("RGA_FZF_INSTANCE").unwrap_or("unk".to_string());

    let mut try_evince = false;
    if files.len() == 1 && files[0].ends_with(".pdf") {
        try_evince = true;
    }
    if try_evince {
        use std::io::ErrorKind::*;

        let worked = Command::new("evince")
            .arg("--find")
            .arg(&query)
            .arg(&files[0])
            .spawn()
            .map_or_else(
                |err| match err.kind() {
                    NotFound => Ok(false),
                    _ => Err(err),
                },
                |_| Ok(true),
            )?;
        if worked {
            return Ok(());
        }
    }
    for f in files.iter() { open::that_detached(f)?; }
    Ok(())
}
