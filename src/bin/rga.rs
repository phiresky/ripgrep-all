use anyhow::Result;
use log::*;
use rga::adapters::spawning::map_exe_error;
use rga::adapters::*;
use rga::args::*;
use rga::matching::*;
use ripgrep_all as rga;
use structopt::StructOpt;

use schemars::schema_for;
use std::process::Command;

fn list_adapters(args: RgaConfig) -> Result<()> {
    let (enabled_adapters, disabled_adapters) = get_all_adapters(args.custom_adapters.clone());

    println!("Adapters:\n");
    let print = |adapter: std::rc::Rc<dyn FileAdapter>| {
        let meta = adapter.metadata();
        let matchers = meta
            .fast_matchers
            .iter()
            .map(|m| match m {
                FastMatcher::FileExtension(ext) => format!(".{}", ext),
            })
            .collect::<Vec<_>>()
            .join(", ");
        let slow_matchers = meta
            .slow_matchers
            .as_ref()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|m| match m {
                SlowMatcher::MimeType(x) => Some(format!("{}", x)),
                SlowMatcher::Fast(_) => None,
            })
            .collect::<Vec<_>>()
            .join(", ");
        let mime_text = if slow_matchers.is_empty() {
            "".to_owned()
        } else {
            format!("Mime Types: {}", slow_matchers)
        };
        print!(
            " - **{name}**\n     {desc}  \n     Extensions: {matchers}  \n     {mime}  \n",
            name = meta.name,
            desc = meta.description.replace("\n", "\n     "),
            matchers = matchers,
            mime = mime_text
        );
        println!("");
    };
    for adapter in enabled_adapters {
        print(adapter)
    }
    println!("The following adapters are disabled by default, and can be enabled using '--rga-adapters=+pdfpages,tesseract':\n");
    for adapter in disabled_adapters {
        print(adapter)
    }
    return Ok(());
}
fn main() -> anyhow::Result<()> {
    env_logger::init();

    let (args, mut passthrough_args) = split_args()?;

    if args.print_config_schema {
        println!("{}", serde_json::to_string_pretty(&schema_for!(RgaConfig))?);
        return Ok(());
    }
    if args.list_adapters {
        return list_adapters(args);
    }
    if let Some(path) = args.fzf_path {
        if path == "_" {
            // fzf found no result, ignore everything and return
            println!("[no file found]");
            return Ok(());
        }
        passthrough_args.push(std::ffi::OsString::from(&path[1..]));
    }

    if passthrough_args.len() == 0 {
        // rg would show help. Show own help instead.
        RgaConfig::clap().print_help()?;
        println!("");
        return Ok(());
    }

    let adapters = get_adapters_filtered(args.custom_adapters.clone(), &args.adapters)?;

    let pre_glob = if !args.accurate {
        let extensions = adapters
            .iter()
            .flat_map(|a| &a.metadata().fast_matchers)
            .flat_map(|m| match m {
                FastMatcher::FileExtension(ext) => vec![ext.clone(), ext.to_ascii_uppercase()],
            })
            .collect::<Vec<_>>()
            .join(",");
        format!("*.{{{}}}", extensions)
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

    let mut child = Command::new("rg")
        .args(rg_args)
        .arg("--pre")
        .arg(preproc_exe)
        .arg("--pre-glob")
        .arg(pre_glob)
        .args(passthrough_args)
        .spawn()
        .map_err(|e| map_exe_error(e, "rg", "Please make sure you have ripgrep installed."))?;

    child.wait()?;
    Ok(())
}

/// add the directory that contains `rga` to PATH, so rga-preproc can find pandoc etc (if we are on Windows where we include dependent binaries)
fn add_exe_to_path() -> Result<()> {
    use std::env;
    let mut exe = env::current_exe().expect("Could not get executable location");
    // let preproc_exe = exe.with_file_name("rga-preproc");
    exe.pop(); // dirname

    let path = env::var_os("PATH").unwrap_or("".into());
    let paths = env::split_paths(&path).collect::<Vec<_>>();
    // prepend: prefer bundled versions to system-installed versions of binaries
    // solves https://github.com/phiresky/ripgrep-all/issues/32
    // may be somewhat of a security issue if rga binary is in installed in unprivileged locations
    let paths = [&[exe.to_owned(), exe.join("lib")], &paths[..]].concat();
    let new_path = env::join_paths(paths)?;
    env::set_var("PATH", &new_path);
    Ok(())
}
