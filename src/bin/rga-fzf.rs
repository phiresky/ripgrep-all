use anyhow::Context;
use rga::adapters::custom::map_exe_error;
use ripgrep_all as rga;

use std::process::{Command, Stdio};

// TODO: add --rg-params=..., --rg-preview-params=... and --fzf-params=... params
// TODO: remove passthrough_args
fn main() -> anyhow::Result<()> {
    env_logger::init();
    let mut passthrough_args: Vec<String> = std::env::args().skip(1).collect();
    let inx = passthrough_args.iter().position(|e| !e.starts_with('-'));
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
    let open_exe = exe.with_file_name("rga-fzf-open");
    let open_exe = open_exe
        .to_str()
        .context("rga-fzf-open executable is in non-unicode path")?;

    let rg_prefix = format!("{preproc_exe} --files-with-matches --rga-cache-max-blob-len=10M");

    let child = Command::new("fzf")
        .arg(format!(
            "--preview={preproc_exe} --pretty --context 5 {{q}} --rga-fzf-path=_{{}}"
        ))
        .arg("--layout=reverse-list")
        .arg("--preview-window=up,60%:wrap,border-bottom")
        .arg("--phony")
        .arg("--query")
        .arg(&initial_query)
        .arg("--print-query")
        .arg(format!("--bind=change:reload: {rg_prefix} {{q}}"))
        .arg(format!("--bind=ctrl-m:execute:{open_exe} {{q}} {{}}"))
        .env(
            "FZF_DEFAULT_COMMAND",
            format!("{} '{}'", rg_prefix, &initial_query),
        )
        .env("RGA_FZF_INSTANCE", format!("{}", std::process::id())) // may be useful to open stuff in the same tab
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| map_exe_error(e, "fzf", "Please make sure you have fzf installed."))?;

    let output = child.wait_with_output()?;
    let mut x = output.stdout.split(|e| e == &b'\n');
    let final_query =
        std::str::from_utf8(x.next().context("fzf output empty")?).context("fzf query not utf8")?;
    let selected_file = std::str::from_utf8(x.next().context("fzf output not two line")?)
        .context("fzf ofilename not utf8")?;
    println!("query='{final_query}', file='{selected_file}'");

    Ok(())
}
