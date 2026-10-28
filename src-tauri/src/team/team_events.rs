use serde::{Deserialize, Serialize};
use specta::Type;
use tauri_specta::Event;

/// The team changed: a run, a member or a line between members. The screen reads the snapshot again.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct TeamChanged {}
