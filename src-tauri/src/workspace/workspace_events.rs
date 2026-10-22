use serde::{Deserialize, Serialize};
use specta::Type;
use tauri_specta::Event;

/// A repository got a worktree the screen did not make itself, so the screen reads its list again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeCreated {
    pub repo_id: String,
}
