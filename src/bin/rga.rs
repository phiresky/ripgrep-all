use clap::{crate_version, App, Arg, SubCommand};
use log::*;
use rga::adapters::*;
use std::ffi::OsString;
use std::process::Command;

fn main() -> std::io::Result<()> {
    env_logger::init();
    let mut app = App::new(env!("CARGO_PKG_NAME"))
        .version(crate_version!())
        .about(env!("CARGO_PKG_DESCRIPTION"))
        // .setting(clap::AppSettings::ArgRequiredElseHelp)
        .arg(Arg::from_usage(
            "--list-adapters 'Lists all known adapters'",
        ))
        .arg(Arg::from_usage("--adapters=[commaseparated] 'Change which adapters to use and in which priority order (descending)'").require_equals(true))
        .arg(Arg::from_usage("--rg-help 'Show help for ripgrep itself'"))
        .arg(Arg::from_usage("--rg-version 'Show version of ripgrep itself'"));
    app.p.create_help_and_version();
    println!("g={:#?},f={:#?}", app.p.groups, app.p.flags);
    for opt in app.p.opts() {
        println!("opt {:#?}", opt.s.long);
    }
    //
    let mut firstarg = true;
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
                        if arg.starts_with(&format!("--{}-", l)) {
                            return true;
                        }
                    }
                }
            }
            false
        });
    debug!("our_args: {:?}", our_args);
    let matches = app.get_matches_from(our_args);
    if matches.is_present("rg-help") {
        passthrough_args.insert(0, "--help".into());
    }
    if matches.is_present("rg-version") {
        passthrough_args.insert(0, "--version".into());
    }
    debug!("passthrough_args: {:?}", passthrough_args);

    let adapters = get_adapters();

    if matches.is_present("list-adapters") {
        println!("Adapters:");
        for adapter in adapters {
            let meta = adapter.metadata();
            println!("{} v{}", meta.name, meta.version);
        }
        return Ok(());
    }

    let extensions = adapters
        .iter()
        .flat_map(|a| &a.metadata().matchers)
        .filter_map(|m| match m {
            Matcher::FileExtension(ext) => Some(ext as &str),
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
        .spawn()?;

    child.wait()?;
    Ok(())
}
