# jb-rs

[![](https://github.com/HBcao233/jb-rs/actions/workflows/build.yml/badge.svg)](https://github.com/HBcao233/jb-rs/actions/workflows/build.yml)
[![](https://github.com/HBcao233/jb-rs/actions/workflows/release.yml/badge.svg)](https://github.com/HBcao233/jb-rs/actions/workflows/release.yml)

JB Cli.

```shell
cargo build --release

target/release/jb <url>

Usage: jb [Options] <url>

Options:
  -c cookie, --cookie cookie      Use cookie
  -i, --info                      Only show infomation, no download.
  -h, --help                      Show help message.
  -q, --quality                   Choose quality of video.
  --keep-temps                    Do not delete the temporary files.
  --allow-mcdn                    Allows to using mCDN/PCDN links to download.
```

# Supported Platforms
* Bilibili

# Similar projects
* [lux](https://github.com/iawia002/lux)
* [yt-dlp](https://github.com/yt-dlp/yt-dlp)
