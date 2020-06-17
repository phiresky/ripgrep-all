use rga::adapters::*;
use rga::preproc::*;
use rga::print_dur;
use ripgrep_all as rga;

use anyhow::Context;
use log::debug;
use std::{fs::File, time::Instant};

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let mut arg_arr: Vec<std::ffi::OsString> = std::env::args_os().collect();
    let last = arg_arr.pop().expect("No filename specified");
    let config = rga::config::parse_args(arg_arr, true)?;
    //clap::App::new("rga-preproc").arg(Arg::from_usage())
    let path = {
        let filepath = last;
        std::env::current_dir()?.join(&filepath)
    };

    let i = File::open(&path).context("Specified input file not found")?;
    let mut o = std::io::stdout();
    let ai = AdaptInfo {
        inp: Box::new(i),
        filepath_hint: path,
        is_real_file: true,
        line_prefix: "".to_string(),
        archive_recursion_depth: 0,
        config,
    };

    let start = Instant::now();
    let mut oup = rga_preproc(ai).context("during preprocessing")?;
    debug!("finding and starting adapter took {}", print_dur(start));
    std::io::copy(&mut oup, &mut o).context("copying adapter output to stdout")?;
    debug!("running adapter took {} total", print_dur(start));
    Ok(())
}
