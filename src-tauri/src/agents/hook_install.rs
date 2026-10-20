//! What the harness hook installers share: the status the settings screen shows, the error they
//! raise, the atomic write they all use, and the version probe that decides what to write.

use std::fs;
use std::io;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::Serialize;
use specta::Type;

/// A harness version as `[major, minor, patch]`. Arrays of numbers compare in release order.
pub type Version = [u32; 3];

const VERSION_PROBE_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum HookInstallState {
    NotInstalled,
    Partial,
    Installed,
    Error,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HookInstallStatus {
    pub state: HookInstallState,
    /// The file Lynx changes for this harness.
    pub config_path: String,
    /// The events Lynx listens to.
    pub events: Vec<String>,
    pub detail: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum HookInstallError {
    #[error("could not find the Lynx executable to write the hook command")]
    NoExecutable,
    #[error("{path} is not a config file Lynx can read, so it was left unchanged")]
    InvalidConfig { path: String },
    #[error("{path} has no agent file to add the hooks to")]
    NoAgentFiles { path: String },
    #[error("`{command}` failed: {detail}")]
    Command { command: String, detail: String },
    #[error("could not update {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: io::Error,
    },
}

/// The command a harness runs for each event: the app's own executable in relay mode, for one
/// harness. Quoted so a path with spaces or quotes still works in the harness's shell.
pub fn relay_command(executable: &Path, harness: &str) -> String {
    let path = executable.to_string_lossy();
    let quoted = if cfg!(windows) {
        format!("\"{path}\"")
    } else {
        format!("'{}'", path.replace('\'', r"'\''"))
    };
    format!("{quoted} --agent-hook {harness}")
}

pub fn invalid_config(path: &Path) -> HookInstallError {
    HookInstallError::InvalidConfig {
        path: path.to_string_lossy().into_owned(),
    }
}

pub fn io_error(path: &Path, source: io::Error) -> HookInstallError {
    HookInstallError::Io {
        path: path.to_string_lossy().into_owned(),
        source,
    }
}

/// Reads a config file. A missing file is `None`, so the caller decides what empty means.
pub fn read_optional(path: &Path) -> Result<Option<String>, HookInstallError> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(io_error(path, source)),
    }
}

/// Writes through a temp file and a rename, so an interrupted write never leaves half a file
/// behind. The file keeps the permissions it had, since settings can hold secrets.
pub fn write_atomic(path: &Path, text: &str) -> Result<(), HookInstallError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| io_error(path, source))?;
    }
    let temporary = path.with_extension(match path.extension() {
        Some(extension) => format!("{}.tmp", extension.to_string_lossy()),
        None => "tmp".to_owned(),
    });
    fs::write(&temporary, text).map_err(|source| io_error(path, source))?;
    if let Ok(metadata) = fs::metadata(path) {
        // Best effort: a file that cannot take the old permissions is still written.
        let _ = fs::set_permissions(&temporary, metadata.permissions());
    }
    fs::rename(&temporary, path).map_err(|source| io_error(path, source))
}

/// Reads `<program> --version` output, such as `2.1.261 (Claude Code)` or `codex-cli 0.160.1`.
pub fn parse_version(output: &str) -> Option<Version> {
    output
        .split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .find_map(|token| {
            let parts = token
                .split('.')
                .map(|part| part.parse::<u32>().ok())
                .collect::<Option<Vec<u32>>>()?;
            <[u32; 3]>::try_from(parts).ok()
        })
}

/// The harness on the PATH, asked for its version. None when it cannot run or answers too slowly.
pub fn probe_version(program: &str) -> Option<Version> {
    let mut child = Command::new(program)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + VERSION_PROBE_TIMEOUT;
    loop {
        if child.try_wait().ok()?.is_some() {
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut output = String::new();
    std::io::Read::read_to_string(&mut child.stdout.take()?, &mut output).ok()?;
    parse_version(&output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_is_read_from_the_harness_output() {
        assert_eq!(parse_version("2.1.261 (Claude Code)\n"), Some([2, 1, 261]));
        assert_eq!(parse_version("codex-cli 0.160.1"), Some([0, 160, 1]));
        assert_eq!(parse_version("opencode v2.0.18"), Some([2, 0, 18]));
        assert_eq!(parse_version("no version here"), None);
        assert_eq!(parse_version("1.2"), None);
    }

    #[test]
    fn a_write_replaces_the_file_and_leaves_no_temp_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("config.toml");
        fs::write(&path, "old").expect("write");

        write_atomic(&path, "new").expect("write atomically");

        assert_eq!(fs::read_to_string(&path).expect("read"), "new");
        assert!(!dir.path().join("config.toml.tmp").exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_write_keeps_the_file_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("hooks.json");
        fs::write(&path, "{}").expect("write");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).expect("chmod");

        write_atomic(&path, "{\"hooks\":{}}").expect("write atomically");

        let mode = fs::metadata(&path).expect("metadata").permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
