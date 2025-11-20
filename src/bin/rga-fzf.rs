use anyhow::Context;
use clap::Parser;
use rga::adapters::custom::map_exe_error;
use rga::shell::{quote_arg, quote_placeholder, tokenize_shell_args};
use ripgrep_all as rga;

use std::process::{Command, Stdio};

#[derive(Parser, Debug)]
#[command(name = "rga-fzf", about = "Interactive fzf launcher for rga")]
struct FzfLauncherArgs {
    #[arg(value_name = "QUERY", help = "Initial query", default_value_t = String::new())]
    query: String,

    #[arg(long = "rg-params", require_equals = true, value_name = "ARGS", default_value_t = String::new())]
    rg_params: String,

    #[arg(long = "rg-preview-params", require_equals = true, value_name = "ARGS", default_value_t = String::new())]
    rg_preview_params: String,

    #[arg(long = "fzf-params", require_equals = true, value_name = "ARGS", default_value_t = String::new())]
    fzf_params: String,

    #[arg(long = "multi", help = "Enable multi-select and pass all files to opener")]
    multi: bool,

    #[arg(long = "fzf-exe", require_equals = true, value_name = "PATH", default_value_t = String::from("fzf"))]
    fzf_exe: String,

    #[arg(long = "open-cmd", require_equals = true, value_name = "CMD")]
    open_cmd: Option<String>,

    #[arg(long = "preview-window", require_equals = true, value_name = "SPEC", default_value_t = String::from("70%:wrap"))]
    preview_window: String,

    #[arg(long = "expect", require_equals = true, value_name = "KEYS", default_value_t = String::new())]
    expect_keys: String,

    #[arg(long = "initial-dir", require_equals = true, value_name = "PATH")]
    initial_dir: Option<String>,
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let args = FzfLauncherArgs::parse();

    let exe = std::env::current_exe().context("Could not get executable location")?;
    let preproc_exe = exe.with_file_name("rga");
    let preproc_exe = preproc_exe
        .to_str()
        .context("rga executable is in non-unicode path")?;
    let open_exe = exe.with_file_name("rga-fzf-open");
    let open_exe = open_exe
        .to_str()
        .context("rga-fzf-open executable is in non-unicode path")?;

    let rg_prefix = format!("{} --files-with-matches --rga-cache-max-blob-len=10M{}",
        quote_arg(preproc_exe),
        if args.rg_params.is_empty() { String::new() } else { format!(" {}", args.rg_params) }
    );

    let q = quote_arg(&args.query);
    let ph_q = quote_placeholder("{q}");
    let mut preview = String::new();
    preview.push_str("--preview=");
    preview.push_str(&quote_arg(preproc_exe));
    if !args.rg_preview_params.is_empty() {
        let toks = tokenize_shell_args(&args.rg_preview_params);
        for t in toks { preview.push(' '); preview.push_str(&quote_arg(&t)); }
    }
    preview.push_str(" --pretty --context 5 -- ");
    preview.push_str(&ph_q);
    preview.push_str(" --rga-fzf-path=_");
    preview.push_str(&quote_placeholder("{}"));

    let bind_change = if let Some(dir) = args.initial_dir.as_ref() {
        format!("--bind=change:reload:{} -- {} {}", rg_prefix, ph_q, quote_arg(dir))
    } else {
        format!("--bind=change:reload:{} -- {}", rg_prefix, ph_q)
    };
    let opener_cmd = args.open_cmd.as_deref().unwrap_or(open_exe);
    let bind_enter = if args.multi {
        format!("--bind=ctrl-m:execute:{} {} {}",
            quote_arg(opener_cmd), ph_q, quote_placeholder("{+}")
        )
    } else {
        format!("--bind=ctrl-m:execute:{} {} {}",
            quote_arg(opener_cmd), ph_q, quote_placeholder("{}")
        )
    };

    let default_cmd = if let Some(dir) = args.initial_dir.as_ref() {
        format!("{} {} {}", rg_prefix, q, quote_arg(dir))
    } else {
        format!("{} {}", rg_prefix, q)
    };

    let mut cmd = Command::new(&args.fzf_exe);
    cmd.arg(preview)
        .arg(format!("--preview-window={}", args.preview_window))
        .arg("--phony")
        .arg("--query")
        .arg(&args.query)
        .arg("--print-query")
        .arg(bind_change)
        .arg(bind_enter)
        .env("FZF_DEFAULT_COMMAND", default_cmd)
        .env("RGA_FZF_INSTANCE", format!("{}", std::process::id()))
        .stdout(Stdio::piped());

    if !args.expect_keys.is_empty() {
        cmd.arg(format!("--expect={}", args.expect_keys));
    }

    if !args.fzf_params.is_empty() {
        for tok in tokenize_shell_args(&args.fzf_params) { cmd.arg(tok); }
    }

    let child = cmd
        .spawn()
        .map_err(|e| map_exe_error(e, "fzf", "Please make sure you have fzf installed."))?;

    let output = child.wait_with_output()?;
    let out = String::from_utf8(output.stdout).context("fzf output not utf8")?;
    let mut it = out.lines();
    let final_query = it.next().context("fzf output empty")?;

    let mut keys_line: Option<String> = None;
    let mut selections: Vec<String> = Vec::new();
    if !args.expect_keys.is_empty() && let Some(l) = it.next() {
            let keys: Vec<&str> = args.expect_keys.split(',').collect();
            let parts: Vec<&str> = l.split('\t').collect();
            if !l.is_empty() && parts.iter().all(|p| keys.contains(p)) {
                keys_line = Some(l.to_string());
            } else {
                selections.push(l.to_string());
            }
    }
    selections.extend(it.map(|s| s.to_string()));
    selections.retain(|s| !s.is_empty());

    if let Some(k) = keys_line.as_ref() {
        println!("keys='{}'", k);
    }
    println!("query='{}'", final_query);
    for s in selections.iter() { println!("file='{}'", s); }

    Ok(())
}
