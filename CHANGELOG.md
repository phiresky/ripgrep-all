# 0.9.2 (2019-06-17)

-   Fix file ending regex ([#13](https://github.com/phiresky/ripgrep-all/issues/13))
-   Fix decoding of UTF16 with BOM ([#5](https://github.com/phiresky/ripgrep-all/issues/5))

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
