use failure::{format_err, Error};
use rga::adapters::*;
use rga::preproc::*;
use std::env;
use std::fs::File;

fn main() -> Result<(), Error> {
    let path = {
        let filepath = std::env::args_os()
            .skip(1)
            .next()
            .ok_or(format_err!("No filename specified"))?;
        eprintln!("inp fname: {:?}", filepath);
        std::env::current_dir()?.join(&filepath)
    };

    eprintln!("abs path: {:?}", path);

    let ai = AdaptInfo {
        inp: &mut File::open(&path)?,
        filepath_hint: &path,
        oup: &mut std::io::stdout(),
        line_prefix: "",
    };

    let cache_db = match env::var("RGA_NO_CACHE") {
        Ok(ref s) if s.len() > 0 => None,
        Ok(_) | Err(_) => Some(open_cache_db()?),
    };

    rga_preproc(ai, cache_db)
}
