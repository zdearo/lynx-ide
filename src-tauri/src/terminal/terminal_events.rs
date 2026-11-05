use std::sync::Arc;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};
use tauri_specta::Event;

use crate::agents::activity::AgentStatus;
use crate::agents::harness::HarnessId;
use crate::browser::BrowserService;
use crate::orchestration::agent_wake::AgentWakes;

use super::output_channels::OutputChannels;
use super::terminal_session::TerminalInfo;

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

/// A session was started, by the screen or by the backend, so the screen can list it.
#[derive(Debug, Clone, Serialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct TerminalCreated {
    pub session: TerminalInfo,
}

/// A session the backend closed for its own reasons, such as a worker a coordinator released. The screen
/// drops its tab. A close the person asked for is not announced: the screen already dropped that tab.
#[derive(Debug, Clone, Serialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct TerminalClosed {
    pub id: String,
}

/// Where a session reports output, exit and agent status. A trait so tests can observe sessions
/// without a running Tauri app.
pub trait TerminalEventSink: Send + Sync + 'static {
    fn output(&self, id: &str, seq: u32, data: String);
    fn exit(&self, id: &str, exit_code: Option<i32>);
    /// Most sinks do not show agent state, so the default ignores it.
    fn agent_state(&self, _id: &str, _harness: Option<HarnessId>, _status: Option<AgentStatus>) {}
    /// The agent named its conversation, which is what resumes it later.
    fn agent_resume(&self, _id: &str, _harness: HarnessId, _resume_id: &str) {}
    /// A session was started, so the screen can list it whoever asked for it.
    fn created(&self, _session: &TerminalInfo) {}
    /// The backend closed a session itself, so the screen drops it. Nothing by default.
    fn closed(&self, _id: &str) {}
}

pub struct TauriEventSink {
    app: AppHandle,
    /// Output goes to the views that attached a channel; exit and agent state stay events.
    channels: Arc<OutputChannels>,
    /// Told when an agent starts or stops working, so the browser knows how long its control lasts.
    browser: Option<Arc<BrowserService>>,
}

impl TauriEventSink {
    pub fn new(
        app: AppHandle,
        browser: Option<Arc<BrowserService>>,
        channels: Arc<OutputChannels>,
    ) -> Self {
        Self {
            app,
            browser,
            channels,
        }
    }
}

impl TerminalEventSink for TauriEventSink {
    fn output(&self, id: &str, seq: u32, data: String) {
        self.channels.send(id, seq, &data);
    }

    fn exit(&self, id: &str, exit_code: Option<i32>) {
        let _ = TerminalExit {
            id: id.to_owned(),
            exit_code,
        }
        .emit(&self.app);
    }

    fn created(&self, session: &TerminalInfo) {
        let _ = TerminalCreated {
            session: session.clone(),
        }
        .emit(&self.app);
    }

    fn closed(&self, id: &str) {
        if let Some(wakes) = self.app.try_state::<Arc<AgentWakes>>() {
            wakes.closed(id);
        }
        let _ = TerminalClosed { id: id.to_owned() }.emit(&self.app);
    }

    fn agent_state(&self, id: &str, harness: Option<HarnessId>, status: Option<AgentStatus>) {
        if let Some(browser) = &self.browser {
            browser.session_status(id, status == Some(AgentStatus::Working));
        }
        // A coordinator that finished its turn may now take the note a worker left for it.
        if let Some(wakes) = self.app.try_state::<Arc<AgentWakes>>() {
            wakes.status_changed(id);
        }
        let _ = AgentStatusChanged {
            id: id.to_owned(),
            harness,
            status,
        }
        .emit(&self.app);
    }
}
