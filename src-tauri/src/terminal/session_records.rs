//! Writes what the app learns about each terminal session to the database, and passes every
//! event on to the sink the screen reads from.

use std::sync::Arc;

use crate::agents::activity::AgentStatus;
use crate::agents::harness::HarnessId;
use crate::persistence::AppDatabase;

use super::terminal_events::TerminalEventSink;
use super::terminal_session::TerminalInfo;

pub struct SessionRecordSink {
    inner: Arc<dyn TerminalEventSink>,
    database: Arc<AppDatabase>,
}

impl SessionRecordSink {
    pub fn new(inner: Arc<dyn TerminalEventSink>, database: Arc<AppDatabase>) -> Self {
        Self { inner, database }
    }
}

impl TerminalEventSink for SessionRecordSink {
    fn output(&self, id: &str, seq: u32, data: String) {
        self.inner.output(id, seq, data);
    }

    fn exit(&self, id: &str, exit_code: Option<i32>) {
        // A session that cannot be recorded still ends on screen; the record is only for later.
        let _ = self.database.end_terminal_session(id, exit_code);
        self.inner.exit(id, exit_code);
    }

    fn agent_state(&self, id: &str, harness: Option<HarnessId>, status: Option<AgentStatus>) {
        if let Some(harness) = harness {
            let _ = self
                .database
                .set_terminal_harness(id, crate::agents::harness::harness_name(harness));
        }
        self.inner.agent_state(id, harness, status);
    }

    fn agent_resume(&self, id: &str, harness: HarnessId, resume_id: &str) {
        let _ = self.database.set_terminal_resume_id(id, resume_id);
        self.inner.agent_resume(id, harness, resume_id);
    }

    fn created(&self, session: &TerminalInfo) {
        self.inner.created(session);
    }
}
