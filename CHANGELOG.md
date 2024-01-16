# 0.10.5 (2024-01-16)

- return the same exit status as rg

# 0.10.4 (2024-01-16)

- add `--rga-no-prefix-filenames` flag (https://github.com/phiresky/ripgrep-all/issues/154)

# 0.10.3 (2024-01-15)

This was originally supposed to be version 1.0.0, but I don't feel confident enough in the stability to call it that.

Highlights:

- rga is now configurable via a config file (~/.config/ripgrep-all/config.jsonc) that is generated on first use, including schema.
- Custom subprocess-spawning adapters can be defined via config file. See https://github.com/phiresky/ripgrep-all/wiki
- External adapters can be shared with the community at https://github.com/phiresky/ripgrep-all/discussions

Others:

- mbox adapter (@FliegendeWurst https://github.com/phiresky/ripgrep-all/pull/104)
- auto generate parts of the readme
- add loads of debug logs and performance timings when `--debug` is used
- better error messages via `anyhow`
- add cross-platform rga-fzf binary
- change whole code base to be async
- change adapter interface from `(&Read, &Write) -> ()` to `AsyncRead -> AsyncRead` to allow chaining of adapters

# 0.9.6 (2020-05-19)

- Fix windows builds
- Case insensitive file extension matching
- Move to Github Actions instead of Travis
- Fix searching for words that are hyphenated in PDFs (#44)
- Always load rga-preproc binary from location where rga is

# 0.9.5 (2020-04-08)

- Allow search in pdf files without extension (https://github.com/phiresky/ripgrep-all/issues/39)
- Prefer shipped binaries to system-installed ones (https://github.com/phiresky/ripgrep-all/issues/32)
- Upgrade dependencies

# 0.9.3 (2019-09-19)

- Fix compilation on new Rust by updating rusqlite ([#25](https://github.com/phiresky/ripgrep-all/pull/25))

# 0.9.2 (2019-06-17)

- Fix file ending regex ([#13](https://github.com/phiresky/ripgrep-all/issues/13))
- Fix decoding of UTF16 with BOM ([#5](https://github.com/phiresky/ripgrep-all/issues/5))
- Shorten the output on failure to two lines (https://github.com/phiresky/ripgrep-all/issues/7), you can use `--no-messages` to completely suppress errors.
- Better installations instructions in readme for each OS
- Add windows binaries! Including all dependencies!

# 0.9.1 (2019-06-16)

- Add enabled adapters to cache key if caching for archive
- Prevent empty trailing page output in pdf reader

# 0.9.0 (2019-06-16)

- Split decompress and tar adapter so we can also read pure .bz2 files etc
- Add mime type detection to decompress so we can read e.g. /boot/initramfs.img which is a bz2 file without ending

# 0.8.9 (2019-06-15)

- Finally fix linux binary package
- add readme to crates.io

# 0.8.7 (2019-06-15)

Minor fixes

- Correctly wrap help text
- Show own help when no arguments given
- Hopefully package the rga binary correctly

# 0.8.5

previous changes not documented
