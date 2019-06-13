# rga - ripgrep, but also search in PDFs, E-Books, Office documents, zip, tar.gz, etc

rga is a line-oriented search tool that allows you to look for a regex in a multitude of file types. It is a wrapper around the awesome [ripgrep] that enables it to search in pdf, docx, pptx, movie subtitles (mkv, mp4), sqlite, etc.

[![Linux build status](https://api.travis-ci.org/phiresky/ripgrep_all.svg)](https://travis-ci.org/phiresky/ripgrep_all)
[![Crates.io](https://img.shields.io/crates/v/ripgrep_all.svg)](https://crates.io/crates/ripgrep_all)

## Future Work

- I wanted to add a photograph adapter (based on object classification / detection) for fun, based on something . It worked with [YOLO](https://pjreddie.com/darknet/yolo/), but something more useful and state-of-the art [like this](https://github.com/aimagelab/show-control-and-tell) proved very hard to integrate.
- 7z adapter (couldn't find a nice to use Rust library)
- allow per-adapter configuration options (probably via env (RGA_ADAPTER_CONF=json))

## Examples

Say you have a large folder of papers or lecture slides, and you can't remember which one of them mentioned `LSTM`s. With rga, you can just run this:

```
rga "LSTM|GRU" collection/
[results]
```

and it will recursively find a regex in pdfs and pptx slides, including if some of them are zipped up.

You can do mostly the same thing with [`pdfgrep -r`][pdfgrep], but it will be much slower and you will miss content in other file types.

```barchart
title: Searching in 20 pdfs with 100 slides each
subtitle: lower is better
data:
   - pdfgrep: 123s
   - rga (first run): 10.3s
   - rga (subsequent runs): 0.1s
```

On the first run rga is mostly faster because of multithreading, but on subsequent runs (on the same files but with any query) rga will cache the text extraction because pdf parsing is slow.

## Setup

rga should compile with stable Rust. To install it, simply run (your OSes equivalent of)

```bash
apt install build-essential pandoc poppler-utils
cargo install ripgrep_all

rga --help # works! :)
```

## Technical details

`rga` simply runs ripgrep (`rg`) with some options set, especially `--pre=rga-preproc` and `--pre-glob`.

`rga-preproc [fname]` will match an adapter to the given file based on either it's filename or it's mime type (if `--accurate` is given).

Some rga adapters run external binaries

## Development

To enable debug logging:

```bash
export RUST_LOG=debug
export RUST_BACKTRACE=1
```

Also rember to disable caching with `--rga-no-cache` or clear the cache in `~/.cache/rga` to debug the adapters.

# Similar tools

- [pdfgrep][pdfgrep]
- [this gist](https://gist.github.com/phiresky/5025490526ba70663ab3b8af6c40a8db) has my proof of concept version of a caching extractor to use ripgrep as a replacement for pdfgrep.
- [this gist](https://gist.github.com/ColonolBuendia/314826e37ec35c616d70506c38dc65aa) is a more extensive preprocessing script by [@ColonolBuendia](https://github.com/ColonolBuendia)

[pdfgrep]: https://pdfgrep.org/
[ripgrep]: https://github.com/BurntSushi/ripgrep
