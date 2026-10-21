//! What the update screen shows, and the rules that decide it. Nothing here touches the network.

use serde::{Deserialize, Serialize};
use specta::Type;

/// Where an update stands. The screen shows each case in its own words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum UpdatePhase {
    /// This copy cannot replace itself, so Lynx does not look for updates.
    Unsupported,
    /// Nothing has been checked yet, or checks are off.
    Idle,
    Checking,
    UpToDate,
    Available {
        version: String,
        notes: Option<String>,
        /// RFC 3339, when the release was published.
        date: Option<String>,
    },
    Downloading {
        version: String,
        /// `None` while the size of the download is unknown.
        percent: Option<u32>,
    },
    /// Installed. The new version runs after a restart.
    ReadyToRestart {
        version: String,
    },
    /// The last check or install failed. The reason is in the log.
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub current_version: String,
    /// RFC 3339, when the last check finished.
    pub checked_at: Option<String>,
    pub phase: UpdatePhase,
}

/// Whether this copy can replace itself. Linux only does it for the AppImage: a .deb or .rpm is
/// owned by the package manager, and a development build has no installed file to replace.
pub fn can_self_update(os: &str, running_from_appimage: bool) -> bool {
    match os {
        "linux" => running_from_appimage,
        _ => true,
    }
}

/// Share of a download that has arrived, when the total size is known.
pub fn percent(received: u64, total: Option<u64>) -> Option<u32> {
    let total = total.filter(|total| *total > 0)?;
    let share = received.saturating_mul(100) / total;
    Some(share.min(100) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_updates_only_from_the_appimage() {
        assert!(can_self_update("linux", true));
        assert!(!can_self_update("linux", false));
        assert!(can_self_update("windows", false));
        assert!(can_self_update("macos", false));
    }

    #[test]
    fn percent_is_known_only_with_a_total() {
        assert_eq!(percent(50, Some(200)), Some(25));
        assert_eq!(percent(50, None), None);
        assert_eq!(percent(50, Some(0)), None);
        assert_eq!(percent(300, Some(200)), Some(100));
    }
}
