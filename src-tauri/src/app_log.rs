//! The app's log: one file, `LYNX_HOME/logs/lynx.log`, that every `log::info!` and `log::error!`
//! in the app writes to. Never put a token, an authorization code or a verifier in it.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::sync::Mutex;

use log::{Level, LevelFilter, Log, Metadata, Record};

pub const FILE_NAME: &str = "lynx.log";
/// Past this size the file is moved aside at startup, so it never grows without limit.
const MAX_BYTES: u64 = 5 * 1024 * 1024;

struct FileLog {
    file: Mutex<File>,
}

impl Log for FileLog {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Info
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format!(
            "{} {:<5} {}: {}\n",
            chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ"),
            record.level(),
            record.target(),
            record.args()
        );
        // A failed write must not take the app down: the log records, it is not needed to run.
        if let Ok(mut file) = self.file.lock() {
            let _ = file.write_all(line.as_bytes());
        }
    }

    fn flush(&self) {}
}

/// Starts the log under `home`. A second call keeps the logger the first one set up.
pub fn init(home: &Path) -> io::Result<()> {
    let dir = home.join("logs");
    fs::create_dir_all(&dir)?;
    let file = dir.join(FILE_NAME);
    roll_over(&file, MAX_BYTES)?;
    let handle = OpenOptions::new().create(true).append(true).open(&file)?;
    if log::set_boxed_logger(Box::new(FileLog {
        file: Mutex::new(handle),
    }))
    .is_ok()
    {
        log::set_max_level(LevelFilter::Info);
    }
    Ok(())
}

/// Moves a log that grew past `max` bytes aside as `lynx.1.log`, replacing the one before it.
fn roll_over(file: &Path, max: u64) -> io::Result<()> {
    if fs::metadata(file).is_ok_and(|meta| meta.len() > max) {
        fs::rename(file, file.with_extension("1.log"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_log_past_the_limit_is_moved_aside() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join(FILE_NAME);
        fs::write(&file, "0123456789").expect("write");

        roll_over(&file, 4).expect("roll over");

        assert!(!file.exists());
        assert_eq!(
            fs::read_to_string(dir.path().join("lynx.1.log")).expect("moved"),
            "0123456789"
        );
    }

    #[test]
    fn a_log_within_the_limit_stays_where_it_is() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join(FILE_NAME);
        fs::write(&file, "0123").expect("write");

        roll_over(&file, 4).expect("roll over");

        assert_eq!(fs::read_to_string(&file).expect("still there"), "0123");
        assert!(!dir.path().join("lynx.1.log").exists());
    }
}
