//! `lynx-pty-daemon --dir <folder> [--idle-secs <n>]`

use std::path::PathBuf;
use std::time::Duration;

use lynx_pty_daemon::server::{DaemonError, Options, serve};

fn main() {
    let mut dir = None;
    let mut idle = 10;
    let mut arguments = std::env::args().skip(1);
    while let Some(flag) = arguments.next() {
        match flag.as_str() {
            "--dir" => dir = arguments.next().map(PathBuf::from),
            "--idle-secs" => {
                idle = arguments
                    .next()
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(idle)
            }
            _ => {}
        }
    }
    let Some(dir) = dir else {
        eprintln!("usage: lynx-pty-daemon --dir <folder> [--idle-secs <n>]");
        std::process::exit(2);
    };
    let options = Options {
        idle: Duration::from_secs(idle),
    };
    match serve(&dir, &options) {
        Ok(()) | Err(DaemonError::AlreadyRunning) => {}
        Err(error) => {
            eprintln!("lynx-pty-daemon: {error}");
            std::process::exit(1);
        }
    }
}
