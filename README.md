# rga - ripgrep, but also search in PDFs, E-Books, Office documents, zip, tar.gz, etc

rga is a tool to recursively search for text in many different types of files. It is based on the awesome [ripgrep](https://github.com/BurntSushi/ripgrep).

[![Linux build status](https://api.travis-ci.org/phiresky/ripgrep_all.svg)](https://travis-ci.org/phiresky/ripgrep_all)
[![Crates.io](https://img.shields.io/crates/v/ripgrep_all.svg)](https://crates.io/crates/ripgrep_all)

# todo

- jpg adapter (based on object classification / detection (yolo?)) for fun
- 7z adapter (couldn't find a nice to use rust library)

# considerations

- matching on mime (magic bytes) instead of filename
- allow per-adapter configuration options

# Setup

rga should compile with stable Rust. To install it, simply run

```bash
apt install build-essential pandoc poppler-utils
cargo install ripgrep_all

rga --help
```

Some rga adapters run external binaries

# Development

To enable debug logging:

```bash
export RUST_LOG=debug
export RUST_BACKTRACE=1
```

Also rember to disable caching with `--rga-no-cache` or clear the cache in `~/.cache/rga` to debug the adapters.

# Similar tools

- [pdfgrep](https://pdfgrep.org/)
- [this gist](https://gist.github.com/phiresky/5025490526ba70663ab3b8af6c40a8db) has my proof of concept version of a caching extractor to use ripgrep as a replacement for pdfgrep.
- [this gist](https://gist.github.com/ColonolBuendia/314826e37ec35c616d70506c38dc65aa) is a more extensive preprocessing script by [@ColonolBuendia](https://github.com/ColonolBuendia)
