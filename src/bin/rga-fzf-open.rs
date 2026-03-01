use anyhow::Context;
use clap::Parser;
use std::process::Command;

#[derive(Parser, Debug, Clone)]
#[clap(name = "rga-fzf-open", about = "Open selected file from rga-fzf")]
struct Args {
    #[clap(value_parser)]
    query: String,
    #[clap(value_parser)]
    fname: String,
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let args = Args::parse();
    let query = args.query;
    let fname = args.fname;

    if fname.ends_with(".pdf") {
        use std::io::ErrorKind::*;

        let worked = Command::new("evince")
            .arg("--find")
            .arg(&query)
            .arg(&fname)
            .spawn()
            .map_or_else(
                |err| match err.kind() {
                    NotFound => Ok(false),
                    _ => Err(err).with_context(|| format!("evince launch failed for '{fname}'")),
                },
                |_| Ok(true),
            )?;
        if worked {
            return Ok(());
        }
    }
    Ok(open::that_detached(&fname).with_context(|| format!("opening '{fname}'"))?)
}
