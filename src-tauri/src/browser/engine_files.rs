//! What is installed on disk: the engine's folder under the Lynx home, and whether it is one this
//! app can run. The folder `engine/current` is either a link to `engine/release-<version>`, as the
//! installer makes it, or the folder `bun run browser:engine` assembles for development.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use lynx_browser_protocol::message::PROTOCOL_VERSION;
use serde::{Deserialize, Serialize};
use specta::Type;

pub const HOST_NAME: &str = "lynx-browser-host";
pub const INFO_FILE: &str = "engine.json";
/// The folders the installer owns, next to `current`.
pub const RELEASE_PREFIX: &str = "release-";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum EngineState {
    Missing,
    /// There, and speaking the protocol this app speaks.
    Ready,
    /// There, but built for another protocol: it has to be installed again.
    Outdated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EngineStatus {
    pub state: EngineState,
    /// Which build is installed, when it says.
    pub version: Option<String>,
    /// Where the host program is, or would be.
    pub path: String,
}

pub fn engine_root(lynx_home: &Path) -> PathBuf {
    lynx_home.join("engine")
}

pub fn current_dir(lynx_home: &Path) -> PathBuf {
    engine_root(lynx_home).join("current")
}

pub fn host_path(lynx_home: &Path) -> PathBuf {
    current_dir(lynx_home).join(HOST_NAME)
}

#[derive(Deserialize)]
struct Info {
    version: Option<String>,
    protocol: Option<u32>,
}

/// Reads what is installed. An engine without a recorded protocol, such as one from before the
/// protocol was versioned, counts as outdated.
pub fn read_status(lynx_home: &Path) -> EngineStatus {
    let host = host_path(lynx_home);
    let path = host.to_string_lossy().into_owned();
    if !host.is_file() {
        return EngineStatus {
            state: EngineState::Missing,
            version: None,
            path,
        };
    }
    let info = fs::read_to_string(current_dir(lynx_home).join(INFO_FILE))
        .ok()
        .and_then(|text| serde_json::from_str::<Info>(&text).ok());
    let state = match info.as_ref().and_then(|info| info.protocol) {
        Some(protocol) if protocol == PROTOCOL_VERSION => EngineState::Ready,
        _ => EngineState::Outdated,
    };
    EngineStatus {
        state,
        version: info.and_then(|info| info.version),
        path,
    }
}

/// Removes everything the engine put under the Lynx home: the link or folder `current` and the
/// release folders. A development cache (`cef-*`, `downloads`) is left, since it is not ours.
pub fn remove(lynx_home: &Path) -> io::Result<()> {
    let root = engine_root(lynx_home);
    let current = current_dir(lynx_home);
    match fs::symlink_metadata(&current) {
        Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(&current)?,
        Ok(_) => fs::remove_file(&current)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let Ok(entries) = fs::read_dir(&root) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(RELEASE_PREFIX) || name == "previous-current" {
            fs::remove_dir_all(entry.path())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn install(home: &Path, info: &str) {
        let dir = current_dir(home);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(HOST_NAME), b"").unwrap();
        fs::write(dir.join(INFO_FILE), info).unwrap();
    }

    #[test]
    fn nothing_installed_is_missing() {
        let home = tempfile::tempdir().unwrap();
        let status = read_status(home.path());
        assert_eq!(status.state, EngineState::Missing);
        assert!(status.path.ends_with("engine/current/lynx-browser-host"));
    }

    #[test]
    fn an_engine_that_speaks_this_protocol_is_ready() {
        let home = tempfile::tempdir().unwrap();
        install(
            home.path(),
            &format!(r#"{{"version":"v1","protocol":{PROTOCOL_VERSION}}}"#),
        );

        let status = read_status(home.path());

        assert_eq!(status.state, EngineState::Ready);
        assert_eq!(status.version.as_deref(), Some("v1"));
    }

    #[test]
    fn an_engine_of_another_protocol_or_with_none_recorded_is_outdated() {
        let home = tempfile::tempdir().unwrap();
        install(home.path(), r#"{"version":"v0","protocol":1}"#);
        assert_eq!(read_status(home.path()).state, EngineState::Outdated);

        install(home.path(), r#"{"cef":"154"}"#);
        assert_eq!(read_status(home.path()).state, EngineState::Outdated);

        install(home.path(), "not json");
        assert_eq!(read_status(home.path()).state, EngineState::Outdated);
    }

    #[test]
    fn removing_takes_the_engine_and_leaves_a_development_cache() {
        let home = tempfile::tempdir().unwrap();
        install(home.path(), "{}");
        let root = engine_root(home.path());
        fs::create_dir_all(root.join("release-old")).unwrap();
        fs::create_dir_all(root.join("cef-154")).unwrap();

        remove(home.path()).expect("removed");

        assert_eq!(read_status(home.path()).state, EngineState::Missing);
        assert!(!root.join("release-old").exists());
        assert!(root.join("cef-154").exists());
        remove(home.path()).expect("removing nothing is fine");
    }
}
