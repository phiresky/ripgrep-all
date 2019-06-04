use rga::adapters::*;

use std::error::Error;
use std::fmt;
use std::path::Path;
use tree_magic;

#[derive(Debug)]
struct ShittyError;

impl Error for ShittyError {}

impl fmt::Display for ShittyError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ShittyError")
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let adapters = init_adapters()?;
    //let ad = &adapters[0];
    //let z: &str = &ad.metadata().name;

    // todo: how to make this less indenty?
    match std::env::args().skip(1).next() {
        Some(filepath) => {
            println!("fname: {}", filepath);
            let path = Path::new(&filepath);
            let maybe_filename = path.file_name();
            match maybe_filename {
                Some(filename) => {
                    let result = tree_magic::from_filepath(path);
                    match result {
                        Some(mimetype) => {
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
                        None => Err("file does not exist".into()),
                    }
                }
                None => Err("Empty filename".into()),
            }
        }
        None => Err("No filename specified".into()),
    }
}
