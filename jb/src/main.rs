mod bili;
mod core;

use std::env;
use std::fs;
use std::io;
use std::process::exit;

use tokio::runtime;

use crate::core::{align_left, padding_left};
use crate::core::{curl, ffmpeg};

const NC: &str = "\x1b[0m";
const RED: &str = "\x1b[1;31m";
const GREEN: &str = "\x1b[1;32m";
const BLUE: &str = "\x1b[1;34m";
const CYAN: &str = "\x1b[36m";
const YELLOW: &str = "\x1b[1;33m";

#[derive(Default, Clone, Debug)]
pub struct Options {
    info: bool,
    cookies: Vec<(String, String)>,
}

async fn async_main() {
    let mut args = env::args();
    args.next();
    let mut options = Options::default();
    let mut input = None;

    loop {
        let Some(arg) = args.next() else {
            break;
        };

        if arg.starts_with('-') {
            if arg == "-h" || arg == "--help" {
                println!(
                    "\
{GREEN}Usage{NC}: jb [Options] <url>

{BLUE}Options{NC}:
  -c [cookie], --cookie [cookie]  使用 cookie
  -i, --info                      仅显示信息不进行下载
  -h, --help                      显示此帮助信息
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
                        let cookies = parse_cookies(&cookies);
                        options.cookies = cookies;
                    }
                    _ => {
                        eprintln!("{RED}error{NC}: \"{arg}\" must be followed by a value.");
                        exit(1);
                    }
                }
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
