use failure::Fallible;
use log::*;
use rga::adapters::spawning::map_exe_error;
use rga::adapters::*;
use rga::args::*;

use std::ffi::OsString;
use std::process::Command;
use structopt::StructOpt;

fn split_args() -> Fallible<(RgaArgs, Vec<OsString>)> {
    let mut app = RgaArgs::clap();

    app.p.create_help_and_version();
    let mut firstarg = true;
    // debug!("{:#?}", app.p.flags);
    let (our_args, mut passthrough_args): (Vec<OsString>, Vec<OsString>) = std::env::args_os()
        .partition(|os_arg| {
            if firstarg {
                // hacky, but .enumerate() would be ugly because partition is too simplistic
                firstarg = false;
                return true;
            }
            if let Some(arg) = os_arg.to_str() {
                for flag in app.p.flags() {
                    if let Some(s) = flag.s.short {
                        if arg == format!("-{}", s) {
                            return true;
                        }
                    }
                    if let Some(l) = flag.s.long {
                        if arg == format!("--{}", l) {
                            return true;
                        }
                    }
                    // println!("{}", flag.s.long);
                }
                for opt in app.p.opts() {
                    // only parse --x=... for now
                    if let Some(l) = opt.s.long {
                        if arg.starts_with(&format!("--{}", l)) {
                            return true;
                        }
                    }
                }
            }
            false
        });
    debug!("our_args: {:?}", our_args);
    let matches = parse_args(our_args)?;
    if matches.rg_help {
        passthrough_args.insert(0, "--help".into());
    }
    if matches.rg_version {
        passthrough_args.insert(0, "--version".into());
    }
    debug!("passthrough_args: {:?}", passthrough_args);
    Ok((matches, passthrough_args))
}

fn main() -> Fallible<()> {
    env_logger::init();

    let (args, passthrough_args) = split_args()?;
    let adapters = get_adapters_filtered(&args.adapters)?;

    if args.list_adapters {
        println!("Adapters:\n");
        for adapter in adapters {
            let meta = adapter.metadata();
            let matchers = meta
                .fast_matchers
                .iter()
                .map(|m| match m {
                    FastMatcher::FileExtension(ext) => format!(".{}", ext),
                })
                .collect::<Vec<_>>()
                .join(", ");
            print!(
                " - {}\n     {}\n     Extensions: {}\n",
                meta.name, meta.description, matchers
            );
            println!("");
        }
        return Ok(());
    }

    let extensions = adapters
        .iter()
        .flat_map(|a| &a.metadata().fast_matchers)
        .filter_map(|m| match m {
            FastMatcher::FileExtension(ext) => Some(ext as &str),
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
        .args(passthrough_args)
        .spawn()
        .map_err(|e| map_exe_error(e, "rg", "Please make sure you have ripgrep installed."))?;

    child.wait()?;
    Ok(())
}
