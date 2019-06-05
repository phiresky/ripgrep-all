use rga::adapters::*;

use std::error::Error;
use std::fmt;
use std::path::Path;
use tree_magic;

// lazy error
fn lerr(inp: impl AsRef<str>) -> Box<dyn Error> {
    return inp.as_ref().into();
}

fn main() -> Result<(), Box<dyn Error>> {
    let adapters = init_adapters()?;
    let filepath = std::env::args()
        .skip(1)
        .next()
        .ok_or(lerr("No filename specified"))?;
    println!("fname: {}", filepath);
    let path = Path::new(&filepath);
    let filename = path.file_name().ok_or(lerr("Empty filename"))?;

    let mimetype = tree_magic::from_filepath(path).ok_or(lerr(format!(
        "File {} does not exist",
        filename.to_string_lossy()
    )))?;
    println!("mimetype: {:?}", mimetype);
    let adapter = adapters(FileMeta {
        mimetype,
        lossy_filename: filename.to_string_lossy().to_string(),
    });
    match adapter {
        Some(ad) => {
            println!("adapter: {}", &ad.metadata().name);
            let stdouti = std::io::stdout();
            let mut stdout = stdouti.lock();
            ad.adapt(&filepath, &mut stdout)?;
            Ok(())
        }
        None => {
            eprintln!("no adapter for that file, running cat!");
            let stdini = std::io::stdin();
            let mut stdin = stdini.lock();
            let stdouti = std::io::stdout();
            let mut stdout = stdouti.lock();
            std::io::copy(&mut stdin, &mut stdout)?;
            Ok(())
        }
    }
}
