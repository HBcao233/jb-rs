mod bili;
mod core;

use std::env;
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

#[derive(Copy, Default, Clone, Debug)]
pub struct Options {
    info: bool,
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
  -i, --info   仅显示信息不进行下载
  -h, --help   显示此帮助信息
"
                );
                exit(0);
            }
            if arg == "-i" {
                options.info = true;
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

    bili::crawler_bili(&input, options).await;
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
