# rga - ripgrep, but also search in PDFs, E-Books, Office documents, zip, tar.gz, etc

[![Linux build status](https://travis-ci.org/phiresky/ripgrep_all.svg)](https://travis-ci.org/BurntSushi/ripgrep)
[![Crates.io](https://img.shields.io/crates/v/ripgrep_all.svg)](https://crates.io/crates/ripgrep_all)

similar:

- pdfgrep
- https://gist.github.com/ColonolBuendia/314826e37ec35c616d70506c38dc65aa

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
