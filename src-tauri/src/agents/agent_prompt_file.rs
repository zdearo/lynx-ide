//! A long prompt for an agent goes in a file, not on the command line: a hook's output or a CI log
//! would not fit there, and quoting several lines for every shell is fragile. The agent is told to
//! read the file.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const MAX_BYTES: usize = 500_000;
/// Prompts are read once, right away: anything older than a day is leftover.
const KEEP_FOR: Duration = Duration::from_secs(24 * 3600);

#[derive(Debug, thiserror::Error)]
pub enum PromptFileError {
    #[error("the prompt is too long")]
    TooLong,
    #[error("could not write the prompt file: {0}")]
    Io(#[from] io::Error),
}

fn prompt_dir() -> PathBuf {
    std::env::temp_dir().join("lynx-prompts")
}

/// Removes the files of `dir` that are older than `age`. Failures are ignored: it is housekeeping.
fn remove_older_than(dir: &Path, age: Duration) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let old = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|elapsed| elapsed >= age);
        if old {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// Writes the prompt to a new file in `dir` and returns its path.
pub fn write_in(dir: &Path, prompt: &str) -> Result<PathBuf, PromptFileError> {
    if prompt.len() > MAX_BYTES {
        return Err(PromptFileError::TooLong);
    }
    fs::create_dir_all(dir)?;
    remove_older_than(dir, KEEP_FOR);
    let path = dir.join(format!("prompt-{}.md", uuid::Uuid::new_v4()));
    fs::write(&path, prompt)?;
    Ok(path)
}

/// Writes the prompt to the temp folder and returns its path.
pub fn write(prompt: &str) -> Result<PathBuf, PromptFileError> {
    write_in(&prompt_dir(), prompt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_prompt_is_written_whole_to_a_new_file_each_time() {
        let dir = tempfile::tempdir().expect("temp dir");

        let first = write_in(dir.path(), "Fix this.\nSecond line.").expect("write");
        let second = write_in(dir.path(), "Again.").expect("write");

        assert_ne!(first, second);
        assert_eq!(
            fs::read_to_string(first).expect("read"),
            "Fix this.\nSecond line."
        );
    }

    #[test]
    fn an_oversized_prompt_is_refused() {
        let dir = tempfile::tempdir().expect("temp dir");

        let error = write_in(dir.path(), &"x".repeat(MAX_BYTES + 1)).expect_err("too long");

        assert!(matches!(error, PromptFileError::TooLong));
    }

    #[test]
    fn old_files_are_removed_and_new_ones_are_kept() {
        let dir = tempfile::tempdir().expect("temp dir");
        let old = dir.path().join("prompt-old.md");
        fs::write(&old, "old").expect("write");

        remove_older_than(dir.path(), Duration::ZERO);

        assert!(!old.exists());
        let fresh = write_in(dir.path(), "fresh").expect("write");
        remove_older_than(dir.path(), Duration::from_secs(3600));
        assert!(fresh.exists());
    }
}
