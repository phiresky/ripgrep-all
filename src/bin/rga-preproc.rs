use rga::adapters::*;
use rga::preproc::*;
use ripgrep_all as rga;

use anyhow::Context;
use std::fs::File;

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let mut arg_arr: Vec<std::ffi::OsString> = std::env::args_os().collect();
    let last = arg_arr.pop().expect("No filename specified");
    let args = rga::args::parse_args(arg_arr, true)?;
    //clap::App::new("rga-preproc").arg(Arg::from_usage())
    let path = {
        let filepath = last;
        std::env::current_dir()?.join(&filepath)
    };

    let i = File::open(&path).context("Specified input file not found")?;
    let mut o = std::io::stdout();
    let cache = if args.no_cache {
        None
    } else {
        Some(rga::preproc_cache::open().context("could not open cache")?)
    };
    let ai = AdaptInfo {
        inp: Box::new(i),
        filepath_hint: path,
        is_real_file: true,
        line_prefix: "".to_string(),
        archive_recursion_depth: 0,
        config: PreprocConfig { cache, args },
    };
    let mut oup = rga_preproc(ai).context("during preprocessing")?;
    std::io::copy(&mut oup, &mut o).context("copying adapter output to stdout")?;
    Ok(())
}
