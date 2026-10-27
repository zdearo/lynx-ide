//! The Jev API key. It is a secret, so it lives in its own file, private to its owner, outside
//! `config.json`, and the screen only ever learns whether one is saved.

use std::fs;
use std::io::{self, ErrorKind};
use std::path::Path;

use super::bot_folder::BotError;
use crate::private_file;

const MAX_KEY: usize = 512;

/// The saved key, or None when there is none.
pub fn read(file: &Path) -> Option<String> {
    let key = fs::read_to_string(file).ok()?;
    let key = key.trim();
    (!key.is_empty()).then(|| key.to_owned())
}

/// Saves the key, replacing the old one. A key with spaces or control characters is refused, since it
/// goes into a header.
pub fn save(file: &Path, key: &str) -> Result<(), BotError> {
    let key = key.trim();
    if key.is_empty()
        || key.chars().count() > MAX_KEY
        || key.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(BotError::InvalidKey);
    }
    private_file::write(file, key.as_bytes())?;
    Ok(())
}

/// Forgets the key. Having none already is fine.
pub fn clear(file: &Path) -> io::Result<()> {
    match fs::remove_file(file) {
        Err(error) if error.kind() != ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_key_is_read_back_trimmed_and_can_be_cleared() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("jev.key");
        assert_eq!(read(&file), None);

        save(&file, "  jev_abc123 \n").expect("saved");
        assert_eq!(read(&file).as_deref(), Some("jev_abc123"));

        clear(&file).expect("cleared");
        clear(&file).expect("clearing twice is fine");
        assert_eq!(read(&file), None);
    }

    #[test]
    fn a_key_that_cannot_go_in_a_header_is_refused() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("jev.key");
        for key in [
            "",
            "   ",
            "two words",
            "line\nbreak",
            &"k".repeat(MAX_KEY + 1),
        ] {
            assert!(
                matches!(save(&file, key), Err(BotError::InvalidKey)),
                "{key:?}"
            );
        }
        assert!(!file.exists());
    }

    #[cfg(unix)]
    #[test]
    fn the_key_file_is_readable_only_by_its_owner() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("jev.key");
        save(&file, "jev_abc123").expect("saved");
        let mode = fs::metadata(&file).expect("file").permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
