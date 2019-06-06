use failure::{format_err, Error};
use path_clean::PathClean;
use rga::adapters::*;
use rga::preproc::*;
use rga::CachingWriter;
use std::fs::File;
use std::path::PathBuf;
use std::rc::Rc;

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

    rga_preproc(ai, Some(open_cache_db()?))
}
