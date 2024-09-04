use anyhow::Result;
use rga::adapters::custom::map_exe_error;
use rga::adapters::*;
use rga::config::{split_args, RgaConfig};
use rga::matching::*;
use rga::print_dur;
use ripgrep_all as rga;
use structopt::StructOpt;

use schemars::schema_for;
use std::process::Command;
use std::time::Instant;

fn list_adapters(args: RgaConfig) -> Result<()> {
    let (enabled_adapters, disabled_adapters) = get_all_adapters(args.custom_adapters);

    println!("Adapters:\n");
    let print = |adapter: std::sync::Arc<dyn FileAdapter>| {
        let meta = adapter.metadata();
        let matchers = meta
            .fast_matchers
            .iter()
            .map(|m| match m {
                FastFileMatcher::FileExtension(ext) => format!(".{ext}"),
            })
            .collect::<Vec<_>>()
            .join(", ");
        let slow_matchers = meta
            .slow_matchers
            .as_ref()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|m| match m {
                FileMatcher::MimeType(x) => Some(x.to_string()),
                FileMatcher::Fast(_) => None,
            })
            .collect::<Vec<_>>()
            .join(", ");
        print!(
            " - **{name}**\n     {desc}  \n     Extensions: {matchers}  \n     Mime Types: {mime}  \n",
            name = meta.name,
            desc = meta.description.replace('\n', "\n     "),
            matchers = matchers,
            mime = slow_matchers,
        );
        println!();
    };
    for adapter in enabled_adapters {
        print(adapter)
    }
    println!("The following adapters are disabled by default, and can be enabled using '--rga-adapters=+foo,bar':\n");
    for adapter in disabled_adapters {
        print(adapter)
    }
    Ok(())
}
fn main() -> anyhow::Result<()> {
    // set debugging as early as possible
    if std::env::args().any(|e| e == "--debug") {
        std::env::set_var("RUST_LOG", "debug");
    }

    env_logger::init();

    let (config, mut passthrough_args) = split_args(false)?;

    if config.print_config_schema {
        println!("{}", serde_json::to_string_pretty(&schema_for!(RgaConfig))?);
        return Ok(());
    }
    if config.list_adapters {
        return list_adapters(config);
    }
    if let Some(path) = config.fzf_path {
        if path == "_" {
            // fzf found no result, ignore everything and return
            println!("[no file found]");
            return Ok(());
        }
        passthrough_args.push(std::ffi::OsString::from(&path[1..]));
    }

    if passthrough_args.is_empty() {
        // rg would show help. Show own help instead.
        RgaConfig::clap().print_help()?;
        println!();
        return Ok(());
    }

    let adapters = get_adapters_filtered(config.custom_adapters.clone(), &config.adapters)?;

    let pre_glob = if !config.accurate {
        let extensions = adapters
            .iter()
            .flat_map(|a| &a.metadata().fast_matchers)
            .flat_map(|m| match m {
                FastFileMatcher::FileExtension(ext) => vec![ext.clone(), ext.to_ascii_uppercase()],
            })
            .collect::<Vec<_>>()
            .join(",");
        format!("*.{{{extensions}}}")
    } else {
        "*".to_owned()
    };

    add_exe_to_path()?;

    let rg_args = vec![
        "--no-line-number",
        // smart case by default because within weird files
        // we probably can't really trust casing anyways
        "--smart-case",
    ];

    let exe = std::env::current_exe().expect("Could not get executable location");
    let preproc_exe = exe.with_file_name("rga-preproc");

    let before = Instant::now();
    let mut cmd = Command::new("rg");
    cmd.args(rg_args)
        .arg("--pre")
        .arg(preproc_exe)
        .arg("--pre-glob")
        .arg(pre_glob)
        .args(passthrough_args);
    log::debug!("rg command to run: {:?}", cmd);
    let mut child = cmd
        .spawn()
        .map_err(|e| map_exe_error(e, "rg", "Please make sure you have ripgrep installed."))?;

    let result = child.wait()?;

    log::debug!("running rg took {}", print_dur(before));
    if !result.success() {
        std::process::exit(result.code().unwrap_or(1));
    }
    Ok(())
}

/// add the directory that contains `rga` to PATH, so rga-preproc can find pandoc etc (if we are on Windows where we include dependent binaries)
fn add_exe_to_path() -> Result<()> {
    use std::env;
    let mut exe = env::current_exe().expect("Could not get executable location");
    // let preproc_exe = exe.with_file_name("rga-preproc");
    exe.pop(); // dirname

    let path = env::var_os("PATH").unwrap_or_default();
    let paths = env::split_paths(&path).collect::<Vec<_>>();
    // prepend: prefer bundled versions to system-installed versions of binaries
    // solves https://github.com/phiresky/ripgrep-all/issues/32
    // may be somewhat of a security issue if rga binary is in installed in unprivileged locations
    let paths = [&[exe.to_owned(), exe.join("lib")], &paths[..]].concat();
    let new_path = env::join_paths(paths)?;
    env::set_var("PATH", new_path);
    Ok(())
}
