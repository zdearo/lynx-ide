//! The agents Lynx supports, and what this computer shows about each: whether the CLI is on the
//! PATH, and whether Lynx's hooks are in its settings. The settings screen lists the ones found
//! here with a switch, and the rest for information.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use specta::Type;

use super::harness::{HarnessId, executable_name, supported};
use super::hook_install::HookInstallStatus;
use super::hook_targets::HookTargets;

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentEntry {
    pub harness: HarnessId,
    /// The CLI is on this computer's PATH.
    pub found: bool,
    /// Lynx's hooks for the agent. Installed means the agent is on: Lynx follows its sessions and
    /// offers it in the new-terminal menu.
    pub hooks: HookInstallStatus,
}

/// Every supported agent, in the order Lynx lists them.
pub fn catalog(targets: &HookTargets) -> Vec<AgentEntry> {
    supported()
        .map(|harness| AgentEntry {
            harness,
            found: on_path(executable_name(harness)),
            hooks: targets.status(harness),
        })
        .collect()
}

/// Whether an executable with this name is on the PATH.
pub fn on_path(program: &str) -> bool {
    env::var_os("PATH").is_some_and(|path| found_in(env::split_paths(&path), program))
}

/// Whether one of the directories holds an executable with this name. A Windows CLI is usually an
/// `.exe` or a `.cmd` shim, so both count there.
pub fn found_in(mut directories: impl Iterator<Item = PathBuf>, program: &str) -> bool {
    directories.any(|directory| {
        let candidates = if cfg!(windows) {
            vec![
                directory.join(format!("{program}.exe")),
                directory.join(format!("{program}.cmd")),
            ]
        } else {
            vec![directory.join(program)]
        };
        candidates.iter().any(|candidate| is_executable(candidate))
    })
}

fn is_executable(path: &Path) -> bool {
    // `metadata` follows symlinks, so a link to the CLI counts.
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    #[test]
    fn an_executable_on_the_path_is_found_and_a_plain_file_is_not() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cli = dir.path().join("fakecli");
        fs::write(&cli, "#!/bin/sh\n").expect("write");
        fs::set_permissions(&cli, fs::Permissions::from_mode(0o755)).expect("chmod");
        let plain = dir.path().join("notes");
        fs::write(&plain, "text").expect("write");
        fs::set_permissions(&plain, fs::Permissions::from_mode(0o644)).expect("chmod");

        assert!(found_in([dir.path().to_path_buf()].into_iter(), "fakecli"));
        assert!(!found_in([dir.path().to_path_buf()].into_iter(), "notes"));
        assert!(!found_in([dir.path().to_path_buf()].into_iter(), "missing"));
    }
}
