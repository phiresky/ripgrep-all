

use failure::{format_err, Fallible};

use rga::adapters::*;
use rga::preproc::*;

use std::fs::File;
use std::io::BufReader;
fn main() -> Fallible<()> {
    env_logger::init();
    let empty: Vec<std::ffi::OsString> = vec![];
    let args = rga::args::parse_args(empty)?;
    //clap::App::new("rga-preproc").arg(Arg::from_usage())
    let path = {
        let filepath = std::env::args_os()
            .skip(1)
            .next()
            .ok_or(format_err!("No filename specified"))?;
        eprintln!("inp fname: {:?}", filepath);
        std::env::current_dir()?.join(&filepath)
    };

    let i = File::open(&path)?;
    let mut o = std::io::stdout();
    let cache = if args.no_cache {
        None
    } else {
        Some(rga::preproc_cache::open()?)
    };
    let ai = AdaptInfo {
        inp: &mut BufReader::new(i),
        filepath_hint: &path,
        is_real_file: true,
        oup: &mut o,
        line_prefix: "",
        archive_recursion_depth: 0,
        config: PreprocConfig {
            cache,
            max_archive_recursion: 3,
        },
    };

    rga_preproc(ai)
}
