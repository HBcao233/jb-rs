//! jb cli.

mod bili;
mod core;

use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::exit;

use tokio::runtime;
use tracing::trace;

use crate::core::{align_left, padding_left, terminal_width};
use crate::core::{curl, ffmpeg};

const NC: &str = "\x1b[0m";
const RED: &str = "\x1b[1;31m";
const GREEN: &str = "\x1b[1;32m";
const BLUE: &str = "\x1b[1;34m";
const CYAN: &str = "\x1b[36m";
const YELLOW: &str = "\x1b[1;33m";

fn home_dir() -> Option<PathBuf> {
    Some(env::home_dir()?.join(".jb"))
}

/// crawler options.
#[derive(Default, Clone, Debug)]
pub struct Options {
    info: bool,
    cookies: Vec<(String, String)>,
    quality: Option<String>,
    keep_temps: bool,
    allow_mcdn: bool,
}

async fn async_main() {
    let mut args = env::args();
    args.next();
    let mut options = Options::default();
    let mut input = None;

    while let Some(arg) = args.next() {
        if arg.starts_with('-') {
            if arg == "-h" || arg == "--help" {
                println!(
                    "\
{GREEN}Usage{NC}: jb [Options] <url>

{BLUE}Options{NC}:
  -c [cookie], --cookie [cookie]  Use cookie
  -i, --info                      Only show infomation, no download.
  -h, --help                      Show this message.
  -q, --quality                   Choose quality of video.
  --keep-temps                    Do not delete the temporary files.
  --allow-mcdn                    Allows to using mCDN/PCDN links to download

{BLUE}tip{NC}: set environment variable `RUST_LOG=log` to enable verbose.
"
                );
                exit(0);
            } else if arg == "-i" || arg == "--info" {
                options.info = true;
            } else if arg == "-c" || arg == "--cookie" || arg == "--cookies" {
                match args.next() {
                    Some(cookie) if !cookie.is_empty() => {
                        let cookies = if cookie.contains('=') {
                            cookie
                        } else {
                            match fs::read_to_string(&cookie) {
                                Ok(text) => text,
                                Err(e) => {
                                    if e.kind() == io::ErrorKind::NotFound {
                                        eprintln!(
                                            "{RED}error{NC}: file \"{cookie}\" not found.\n\
                                            \n\
                                            {BLUE}note{NC}: string without char '=' will be treated as filename."
                                        );
                                    } else {
                                        eprintln!("{RED}error{NC}: reading file failed: {e}");
                                    }
                                    exit(1);
                                }
                            }
                        };
                        let cookies = cookies.trim();
                        let cookies = parse_cookies(cookies);
                        trace!(?cookies, "Use cookies");

                        options.cookies = cookies;
                    }
                    _ => {
                        eprintln!("{RED}error{NC}: \"{arg}\" must be followed by a value.");
                        exit(1);
                    }
                }
            } else if arg == "-q" || arg == "--quality" {
                match args.next() {
                    Some(quality) if !quality.is_empty() => {
                        options.quality = Some(quality);
                    }
                    _ => {
                        eprintln!("{RED}error{NC}: \"{arg}\" must be followed by a value.");
                        exit(1);
                    }
                }
            } else if arg == "--keep-temps" {
                options.keep_temps = true;
            } else if arg == "--allow-mcdn" {
                options.allow_mcdn = true;
            }
        } else {
            if input.is_some() {
                eprintln!("{RED}error{NC}: too more url");
                exit(1);
            } else {
                input = Some(arg);
            }
        }
    }

    let input = input.unwrap_or_else(|| {
        eprintln!(
            "{RED}error{NC}: url not found.\n\n\
             {BLUE}note{NC}: run `jb -h` for help"
        );
        exit(1);
    });

    bili::crawler_bili(&input, &options).await;

    eprintln!("{RED}error{NC}: Unsupported url: \"{input}\".");
    exit(1);
}

fn main() {
    use tracing_subscriber::{EnvFilter, fmt, prelude::*};

    let layer = fmt::layer().without_time();
    tracing_subscriber::registry()
        .with(layer)
        .with(EnvFilter::from_default_env())
        .init();

    runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async_main());
}

fn parse_cookies(cookies: &str) -> Vec<(String, String)> {
    cookies
        .split(";")
        .filter_map(|c| {
            let mut parts = c.trim().splitn(2, '=');
            if let (Some(key), Some(value)) = (parts.next(), parts.next()) {
                Some((key.trim().to_string(), value.trim().to_string()))
            } else {
                None
            }
        })
        .collect()
}
