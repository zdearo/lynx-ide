//! Where the browser engine lives. It is a separate program, installed under the Lynx home by
//! `bun run browser:engine` (and, later, by an optional download), so the app itself stays small.

use std::path::{Path, PathBuf};

use super::browser_error::BrowserError;

const HOST_NAME: &str = "lynx-browser-host";

/// Where the installed engine's host program is, whether or not it exists.
pub fn host_path(lynx_home: &Path) -> PathBuf {
    lynx_home.join("engine").join("current").join(HOST_NAME)
}

pub fn locate(lynx_home: &Path) -> Result<PathBuf, BrowserError> {
    let host = host_path(lynx_home);
    if host.is_file() {
        Ok(host)
    } else {
        Err(BrowserError::EngineMissing(host))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_engine_says_where_it_looked() {
        let home = tempfile::tempdir().unwrap();
        match locate(home.path()) {
            Err(BrowserError::EngineMissing(path)) => {
                assert_eq!(path, home.path().join("engine/current/lynx-browser-host"));
            }
            other => panic!("expected a missing engine, got {other:?}"),
        }
    }

    #[test]
    fn an_installed_engine_is_found() {
        let home = tempfile::tempdir().unwrap();
        let host = host_path(home.path());
        std::fs::create_dir_all(host.parent().unwrap()).unwrap();
        std::fs::write(&host, b"").unwrap();
        assert_eq!(locate(home.path()).unwrap(), host);
    }

    #[test]
    fn a_folder_with_the_host_name_is_not_the_host() {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(host_path(home.path())).unwrap();
        assert!(locate(home.path()).is_err());
    }
}
