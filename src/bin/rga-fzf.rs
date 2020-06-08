use anyhow::Context;
use rga::adapters::spawning::map_exe_error;
use ripgrep_all as rga;

use std::process::{Command, Stdio};

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let mut passthrough_args: Vec<String> = std::env::args().skip(1).collect();
    let inx = passthrough_args.iter().position(|e| !e.starts_with("-"));
    let initial_query = if let Some(inx) = inx {
        passthrough_args.remove(inx)
    } else {
        "".to_string()
    };

    let exe = std::env::current_exe().context("Could not get executable location")?;
    let preproc_exe = exe.with_file_name("rga");
    let preproc_exe = preproc_exe
        .to_str()
        .context("rga executable is in non-unicode path")?;

    let rg_prefix = format!(
        "{} --files-with-matches --rga-cache-max-blob-len=10M",
        preproc_exe
    );

    let child = Command::new("fzf")
        .arg(format!(
            "--preview={} --pretty --context 5 {{q}} --rga-fzf-path=_{{}}",
            preproc_exe
        ))
        .arg("--phony")
        .arg("--query")
        .arg(&initial_query)
        .arg("--print-query")
        .arg(format!("--bind=change:reload: {} {{q}}", rg_prefix))
        .env(
            "FZF_DEFAULT_COMMAND",
            format!("{} '{}'", rg_prefix, &initial_query),
        )
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| map_exe_error(e, "fzf", "Please make sure you have fzf installed."))?;

    let output = child.wait_with_output()?;
    let mut x = output.stdout.split(|e| e == &b'\n');
    let final_query =
        std::str::from_utf8(x.next().context("fzf output empty")?).context("fzf query not utf8")?;
    let selected_file = std::str::from_utf8(x.next().context("fzf output not two line")?)
        .context("fzf ofilename not utf8")?;
    println!("query='{}', file='{}'", final_query, selected_file);

    if selected_file.ends_with(".pdf") {
        use std::io::ErrorKind::*;
        let worked = Command::new("evince")
            .arg("--find")
            .arg(final_query)
            .arg(selected_file)
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
    Command::new("xdg-open").arg(selected_file).spawn()?;

    Ok(())
}
