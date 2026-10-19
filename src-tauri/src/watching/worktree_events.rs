use serde::{Deserialize, Serialize};
use specta::Type;
use tauri_specta::Event;

/// Files changed in the watched worktree. Sent once a burst of changes has settled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeChanged {
    pub worktree_path: String,
    /// Folders whose contents changed, relative to the worktree (`""` is the root).
    pub directories: Vec<String>,
    /// Too many folders changed to list: read everything that is open again.
    pub everything: bool,
    /// Git's own state moved: the index or `HEAD` was written (a stage, a commit, a checkout).
    pub git: bool,
}
