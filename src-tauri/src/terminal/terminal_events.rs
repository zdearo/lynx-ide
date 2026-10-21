use std::sync::Arc;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;
use tauri_specta::Event;

use crate::agents::activity::AgentStatus;
use crate::agents::harness::HarnessId;
use crate::browser::BrowserService;

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

/// What a session is running now: a harness and its status, or nothing for a plain shell.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct AgentStatusChanged {
    pub id: String,
    pub harness: Option<HarnessId>,
    pub status: Option<AgentStatus>,
}

/// Where a session reports output, exit and agent status. A trait so tests can observe sessions
/// without a running Tauri app.
pub trait TerminalEventSink: Send + Sync + 'static {
    fn output(&self, id: &str, seq: u32, data: String);
    fn exit(&self, id: &str, exit_code: Option<i32>);
    /// Most sinks do not show agent state, so the default ignores it.
    fn agent_state(&self, _id: &str, _harness: Option<HarnessId>, _status: Option<AgentStatus>) {}
}

pub struct TauriEventSink {
    app: AppHandle,
    /// Told when an agent starts or stops working, so the browser knows how long its control lasts.
    browser: Option<Arc<BrowserService>>,
}

impl TauriEventSink {
    pub fn new(app: AppHandle, browser: Option<Arc<BrowserService>>) -> Self {
        Self { app, browser }
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

    fn agent_state(&self, id: &str, harness: Option<HarnessId>, status: Option<AgentStatus>) {
        if let Some(browser) = &self.browser {
            browser.session_status(id, status == Some(AgentStatus::Working));
        }
        let _ = AgentStatusChanged {
            id: id.to_owned(),
            harness,
            status,
        }
        .emit(&self.app);
    }
}
