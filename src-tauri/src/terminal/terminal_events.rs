use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;
use tauri_specta::Event;

/// A chunk of terminal output. `seq` counts chunks per session from 1, so a view
/// that attached from a snapshot can skip chunks the snapshot already contains.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct TerminalOutput {
    pub id: String,
    pub seq: u32,
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct TerminalExit {
    pub id: String,
    pub exit_code: Option<i32>,
}

/// Where a session reports output and exit. A trait so tests can observe sessions
/// without a running Tauri app.
pub trait TerminalEventSink: Send + Sync + 'static {
    fn output(&self, id: &str, seq: u32, data: String);
    fn exit(&self, id: &str, exit_code: Option<i32>);
}

pub struct TauriEventSink {
    app: AppHandle,
}

impl TauriEventSink {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl TerminalEventSink for TauriEventSink {
    fn output(&self, id: &str, seq: u32, data: String) {
        // A failed emit means the window is closing; there is no one left to tell.
        let _ = TerminalOutput {
            id: id.to_owned(),
            seq,
            data,
        }
        .emit(&self.app);
    }

    fn exit(&self, id: &str, exit_code: Option<i32>) {
        let _ = TerminalExit {
            id: id.to_owned(),
            exit_code,
        }
        .emit(&self.app);
    }
}
