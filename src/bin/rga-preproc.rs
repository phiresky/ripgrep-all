use rga::adapters::*;
use rga::preproc::*;
use rga::print_dur;
use ripgrep_all as rga;

use anyhow::Context;
use log::debug;
use std::time::Instant;
use tokio::fs::File;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let mut arg_arr: Vec<std::ffi::OsString> = std::env::args_os().collect();
    let last = arg_arr.pop().expect("No filename specified");
    let mut config = rga::config::parse_args(arg_arr, true)?;
    if config.decompress_autotune_import.is_none()
        && let Ok(p) = std::env::var("RGA_DECOMPRESS_AUTOTUNE_IMPORT")
        && !p.is_empty() { config.decompress_autotune_import = Some(p); }
    if let Some(p) = config.decompress_autotune_import.as_ref() { let _ = rga::adapters::decompress::import_caps_from_file(p); }
    //clap::App::new("rga-preproc").arg(Arg::from_usage())
    let path = {
        let filepath = last;
        std::env::current_dir()?.join(filepath)
    };

    let i = File::open(&path)
        .await
        .context("Specified input file not found")?;
    let mut o = tokio::io::stdout();
    let ai = AdaptInfo {
        inp: Box::pin(i),
        filepath_hint: path,
        is_real_file: true,
        line_prefix: "".to_string(),
        archive_recursion_depth: 0,
        postprocess: !config.no_prefix_filenames,
        config: config.clone(),
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
    if config.decompress_autotune_export.is_none()
        && let Ok(p) = std::env::var("RGA_DECOMPRESS_AUTOTUNE_EXPORT")
        && !p.is_empty() { config.decompress_autotune_export = Some(p); }
    if let Some(p) = config.decompress_autotune_export.as_ref() { let _ = rga::adapters::decompress::export_caps_to_file(p); }
    if config.profile { println!("{}", rga::preproc::prof_summary()); }
    Ok(())
}
