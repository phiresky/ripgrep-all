# 0.9.7 (unreleased)

-   auto generate parts of the readme
-   add loads of debug logs and performance timings when `--debug` is used
-   better error messages via `anyhow`
-   add cross-platform rga-fzf binary
-   add a config file including schema

# 0.9.6 (2020-05-19)

-   Fix windows builds
-   Case insensitive file extension matching
-   Move to Github Actions instead of Travis
-   Fix searching for words that are hyphenated in PDFs (#44)
-   Always load rga-preproc binary from location where rga is

# 0.9.5 (2020-04-08)

-   Allow search in pdf files without extension (https://github.com/phiresky/ripgrep-all/issues/39)
-   Prefer shipped binaries to system-installed ones (https://github.com/phiresky/ripgrep-all/issues/32)
-   Upgrade dependencies

# 0.9.3 (2019-09-19)

-   Fix compilation on new Rust by updating rusqlite ([#25](https://github.com/phiresky/ripgrep-all/pull/25))

# 0.9.2 (2019-06-17)

-   Fix file ending regex ([#13](https://github.com/phiresky/ripgrep-all/issues/13))
-   Fix decoding of UTF16 with BOM ([#5](https://github.com/phiresky/ripgrep-all/issues/5))
-   Shorten the output on failure to two lines (https://github.com/phiresky/ripgrep-all/issues/7), you can use `--no-messages` to completely suppress errors.
-   Better installations instructions in readme for each OS
-   Add windows binaries! Including all dependencies!

# 0.9.1 (2019-06-16)

-   Add enabled adapters to cache key if caching for archive
-   Prevent empty trailing page output in pdf reader

# 0.9.0 (2019-06-16)

-   Split decompress and tar adapter so we can also read pure .bz2 files etc
-   Add mime type detection to decompress so we can read e.g. /boot/initramfs.img which is a bz2 file without ending

# 0.8.9 (2019-06-15)

-   Finally fix linux binary package
-   add readme to crates.io

# 0.8.7 (2019-06-15)

Minor fixes

-   Correctly wrap help text
-   Show own help when no arguments given
-   Hopefully package the rga binary correctly

# 0.8.5

previous changes not documented
