use rga::adapters::*;
use rga::preproc::*;
use rga::print_dur;
use ripgrep_all as rga;

use anyhow::Context;
use log::debug;
use std::time::Instant;
use tokio::fs::File;
use tokio::io::BufReader;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let mut arg_arr: Vec<std::ffi::OsString> = std::env::args_os().collect();
    let last = arg_arr.pop().expect("No filename specified");
    let config = rga::config::parse_args(arg_arr, true)?;
    //clap::App::new("rga-preproc").arg(Arg::from_usage())
    let path = {
        let filepath = last;
        std::env::current_dir()?.join(filepath)
    };

    let i = File::open(&path)
        .await
        .context("Specified input file not found")?;
    let i = BufReader::new(i);
    let mut o = tokio::io::stdout();
    let ai = AdaptInfo {
        inp: Box::pin(i),
        filepath_hint: path,
        is_real_file: true,
        line_prefix: "".to_string(),
        archive_recursion_depth: 0,
        postprocess: !config.no_prefix_filenames,
        config,
    };

    let start = Instant::now();
    let mut oup = rga_preproc(ai).await.context("during preprocessing")?;
    debug!("finding and starting adapter took {}", print_dur(start));
    let res = tokio::io::copy(&mut oup, &mut o).await;
    if let Err(e) = res {
        if e.kind() == std::io::ErrorKind::BrokenPipe {
            // happens if e.g. ripgrep detects binary data in the pipe so it cancels reading
            debug!("output cancelled (broken pipe)");
        } else {
            Err(e).context("copying adapter output to stdout")?;
        }
    }
    debug!("running adapter took {} total", print_dur(start));
    Ok(())
}
