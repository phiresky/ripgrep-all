use rga::adapters::*;
use rga::preproc::*;
use ripgrep_all as rga;

use std::fs::File;

fn main() -> Result<(), exitfailure::ExitFailure> {
    env_logger::init();
    let mut arg_arr: Vec<std::ffi::OsString> = std::env::args_os().collect();
    let last = arg_arr.pop().expect("No filename specified");
    let args = rga::args::parse_args(arg_arr)?;
    //clap::App::new("rga-preproc").arg(Arg::from_usage())
    let path = {
        let filepath = last;
        std::env::current_dir()?.join(&filepath)
    };

    let mut i = File::open(&path)?;
    let mut o = std::io::stdout();
    let cache = if args.no_cache {
        None
    } else {
        Some(rga::preproc_cache::open()?)
    };
    let ai = AdaptInfo {
        inp: &mut i,
        filepath_hint: &path,
        is_real_file: true,
        oup: &mut o,
        line_prefix: "",
        archive_recursion_depth: 0,
        config: PreprocConfig { cache, args: &args },
    };
    rga_preproc(ai)?;
    Ok(())
}
