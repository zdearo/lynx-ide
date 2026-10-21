use serde::{Deserialize, Serialize};
use specta::Type;
use tauri_specta::Event;

use super::update_state::UpdateStatus;

/// The update status changed: a check started or ended, or a download moved.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatusChanged {
    pub status: UpdateStatus,
}
