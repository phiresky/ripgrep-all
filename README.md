# rga - ripgrep, but also search in PDFs, E-Books, Office documents, zip, tar.gz, etc

similar:

- pdfgrep
- https://gist.github.com/ColonolBuendia/314826e37ec35c616d70506c38dc65aa

# todo

- jpg adapter (based on object classification / detection (yolo?)) for fun
- 7z adapter (couldn't find a nice to use rust library)

# considerations

- matching on mime (magic bytes) instead of filename
- allow per-adapter configuration options

# Development

To enable debug logging:

```bash
export RUST_LOG=debug
export RUST_BACKTRACE=1
```

Also rember to disable caching with `--rga-no-cache` or clear the cache in `~/.cache/rga` to debug the adapters.
