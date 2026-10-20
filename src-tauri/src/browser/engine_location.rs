//! Where the browser engine lives. It is a separate program under the Lynx home, put there by the
//! in-app install (or `bun run browser:engine` when developing Lynx), so the app itself stays small.

use std::path::{Path, PathBuf};

use super::browser_error::BrowserError;
use super::engine_files::{EngineState, host_path, read_status};

pub fn locate(lynx_home: &Path) -> Result<PathBuf, BrowserError> {
    let host = host_path(lynx_home);
    match read_status(lynx_home).state {
        EngineState::Ready => Ok(host),
        EngineState::Missing => Err(BrowserError::EngineMissing(host)),
        EngineState::Outdated => Err(BrowserError::EngineOutdated(host)),
    }
}

#[cfg(test)]
mod tests {
    use lynx_browser_protocol::message::PROTOCOL_VERSION;

    use super::*;
    use crate::browser::engine_files::{INFO_FILE, current_dir};

    fn install(home: &Path, protocol: u32) -> PathBuf {
        let host = host_path(home);
        std::fs::create_dir_all(host.parent().unwrap()).unwrap();
        std::fs::write(&host, b"").unwrap();
        std::fs::write(
            current_dir(home).join(INFO_FILE),
            format!(r#"{{"version":"v","protocol":{protocol}}}"#),
        )
        .unwrap();
        host
    }

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
        let host = install(home.path(), PROTOCOL_VERSION);
        assert_eq!(locate(home.path()).unwrap(), host);
    }

    #[test]
    fn an_engine_of_another_protocol_is_outdated_not_started() {
        let home = tempfile::tempdir().unwrap();
        install(home.path(), PROTOCOL_VERSION + 1);
        assert!(matches!(
            locate(home.path()),
            Err(BrowserError::EngineOutdated(_))
        ));
    }

    #[test]
    fn a_folder_with_the_host_name_is_not_the_host() {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(host_path(home.path())).unwrap();
        assert!(locate(home.path()).is_err());
    }
}
