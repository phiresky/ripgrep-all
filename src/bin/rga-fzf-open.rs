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
    let mut fname = args.fname;

    let mut page = None;
    if let Some(caps) = regex::Regex::new(r"Page (\d+): (.*)").unwrap().captures(&fname) {
        page = Some(caps.get(1).unwrap().as_str().to_string());
        fname = caps.get(2).unwrap().as_str().to_string();
    }

    if fname.ends_with(".pdf") {
        use std::io::ErrorKind::*;

        let mut cmd = Command::new("evince");
        if let Some(p) = page {
            cmd.arg("--page-label").arg(p);
        }
        let worked = cmd
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
    open::that_detached(&fname).with_context(|| format!("opening '{fname}'"))
}
