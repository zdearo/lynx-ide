//! Writes what the app learns about each terminal session to the database, and passes every
//! event on to the sink the screen reads from.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use crate::agents::activity::AgentStatus;
use crate::agents::harness::HarnessId;
use crate::agents::session_title::{self, TitleWatch};
use crate::persistence::AppDatabase;

use super::terminal_events::TerminalEventSink;
use super::terminal_session::TerminalInfo;

pub struct SessionRecordSink {
    inner: Arc<dyn TerminalEventSink>,
    database: Arc<AppDatabase>,
    /// The name watch of each session that runs an agent. Dropping one stops it.
    titles: Mutex<HashMap<String, TitleWatch>>,
}

impl SessionRecordSink {
    pub fn new(inner: Arc<dyn TerminalEventSink>, database: Arc<AppDatabase>) -> Self {
        Self {
            inner,
            database,
            titles: Mutex::new(HashMap::new()),
        }
    }

    /// Follows the name the agent keeps for this conversation. A new conversation in the same session
    /// replaces the old watch.
    fn follow_title(&self, id: &str, harness: HarnessId, resume_id: &str) {
        let database = Arc::clone(&self.database);
        let inner = Arc::clone(&self.inner);
        let session = id.to_owned();
        let watch = session_title::watch_title(harness, resume_id, move |title| {
            let _ = database.set_terminal_agent_title(&session, &title);
            inner.title_changed(&session, &title);
        });
        let watch = watch.unwrap_or_else(|error| {
            log::error!("could not follow the agent title of {id}: {error}");
            None
        });
        let previous = {
            let mut titles = self.titles.lock().unwrap_or_else(PoisonError::into_inner);
            match watch {
                Some(watch) => titles.insert(id.to_owned(), watch),
                None => titles.remove(id),
            }
        };
        drop(previous);
    }

    fn forget_title(&self, id: &str) {
        let previous = self
            .titles
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(id);
        drop(previous);
    }
}

impl TerminalEventSink for SessionRecordSink {
    fn output(&self, id: &str, seq: u32, data: String) {
        self.inner.output(id, seq, data);
    }

    fn exit(&self, id: &str, exit_code: Option<i32>) {
        // A session that cannot be recorded still ends on screen; the record is only for later.
        let _ = self.database.end_terminal_session(id, exit_code);
        // shortcut: a title written in the last instant before the exit may miss the watch; a restored
        // session follows again when its agent reports the conversation.
        self.forget_title(id);
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
        self.follow_title(id, harness, resume_id);
        self.inner.agent_resume(id, harness, resume_id);
    }

    fn created(&self, session: &TerminalInfo) {
        self.inner.created(session);
    }

    fn closed(&self, id: &str) {
        self.forget_title(id);
        self.inner.closed(id);
    }

    fn title_changed(&self, id: &str, title: &str) {
        self.inner.title_changed(id, title);
    }
}
