//! Where the app's hub listens, kept in a file as well as in each session's environment. The
//! environment is fixed when a session starts, so an agent that outlives the app's run would
//! keep a port and token nobody listens on; the file always names the current ones.

use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::hook_hub::{FILE_ENV, PORT_ENV, TOKEN_ENV};
use crate::private_file;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HubAddress {
    pub port: u16,
    pub token: String,
}

/// The file holds the hub's token, so it is written private to its owner.
pub fn publish(file: &Path, address: &HubAddress) -> io::Result<()> {
    private_file::write(file, serde_json::to_string(address)?.as_bytes())
}

/// None when the file is missing or is not an address, so the caller falls back.
pub fn read(file: &Path) -> Option<HubAddress> {
    serde_json::from_str(&fs::read_to_string(file).ok()?).ok()
}

/// The address a hook relay or MCP process should use right now. Read on every call, because an
/// MCP process lives as long as its agent and the app can restart under it.
pub fn current() -> Option<HubAddress> {
    resolve(
        std::env::var_os(FILE_ENV).as_deref().map(Path::new),
        std::env::var(PORT_ENV).ok().as_deref(),
        std::env::var(TOKEN_ENV).ok().as_deref(),
    )
}

/// The file wins over the variables: they are what the session started with, the file is what the
/// app says now. Sessions from before the file existed only have the variables.
fn resolve(file: Option<&Path>, port: Option<&str>, token: Option<&str>) -> Option<HubAddress> {
    file.and_then(read).or_else(|| {
        Some(HubAddress {
            port: port?.parse().ok()?,
            token: token?.to_owned(),
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn address(port: u16, token: &str) -> HubAddress {
        HubAddress {
            port,
            token: token.to_owned(),
        }
    }

    #[test]
    fn a_published_address_reads_back() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("hub.json");

        publish(&file, &address(4021, "secret")).expect("publish");

        assert_eq!(read(&file), Some(address(4021, "secret")));
    }

    #[test]
    fn publishing_again_replaces_the_address_and_leaves_no_temporary_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("hub.json");
        publish(&file, &address(1, "old")).expect("publish");

        publish(&file, &address(2, "new")).expect("publish again");

        assert_eq!(read(&file), Some(address(2, "new")));
        let names: Vec<_> = fs::read_dir(dir.path())
            .expect("list")
            .map(|entry| entry.expect("entry").file_name())
            .collect();
        assert_eq!(names, ["hub.json"]);
    }

    #[test]
    fn the_folder_is_created_when_it_does_not_exist() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("not").join("yet").join("hub.json");

        publish(&file, &address(7, "t")).expect("publish");

        assert_eq!(read(&file), Some(address(7, "t")));
    }

    #[test]
    fn a_missing_or_broken_file_reads_as_nothing() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("hub.json");
        assert_eq!(read(&file), None);

        fs::write(&file, "not json").expect("write");
        assert_eq!(read(&file), None);

        fs::write(&file, r#"{"port":"high","token":"t"}"#).expect("write");
        assert_eq!(read(&file), None);
    }

    #[cfg(unix)]
    #[test]
    fn only_the_owner_can_read_the_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("hub.json");

        publish(&file, &address(1, "t")).expect("publish");

        let mode = fs::metadata(&file).expect("metadata").permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn the_file_wins_over_the_variables_a_session_started_with() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("hub.json");
        publish(&file, &address(5000, "fresh")).expect("publish");

        let found = resolve(Some(&file), Some("4000"), Some("stale"));

        assert_eq!(found, Some(address(5000, "fresh")));
    }

    #[test]
    fn the_variables_are_used_when_there_is_no_usable_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let missing = dir.path().join("hub.json");
        let broken = dir.path().join("broken.json");
        fs::write(&broken, "{").expect("write");
        let expected = Some(address(4000, "from-env"));

        assert_eq!(resolve(None, Some("4000"), Some("from-env")), expected);
        assert_eq!(
            resolve(Some(&missing), Some("4000"), Some("from-env")),
            expected
        );
        assert_eq!(
            resolve(Some(&broken), Some("4000"), Some("from-env")),
            expected
        );
    }

    #[test]
    fn without_a_file_or_complete_variables_there_is_no_address() {
        assert_eq!(resolve(None, None, None), None);
        assert_eq!(resolve(None, Some("4000"), None), None);
        assert_eq!(resolve(None, Some("high"), Some("t")), None);
    }
}
