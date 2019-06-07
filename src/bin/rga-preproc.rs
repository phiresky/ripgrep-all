use failure::{format_err, Error};
use rga::adapters::*;
use rga::preproc::*;
use std::env;
use std::fs::File;
use std::io::BufReader;

fn main() -> Result<(), Error> {
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
    let ai = AdaptInfo {
        inp: &mut BufReader::new(i),
        filepath_hint: &path,
        is_real_file: true,
        oup: &mut o,
        line_prefix: "",
        archive_recursion_depth: 0,
    };

    let cache_db = match env::var("RGA_NO_CACHE") {
        Ok(ref s) if s.len() > 0 => None,
        Ok(_) | Err(_) => Some(open_cache_db()?),
    };

    rga_preproc(ai, cache_db)
}
