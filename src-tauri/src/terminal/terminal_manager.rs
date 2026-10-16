use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use super::shell_resolution::Shell;
use super::terminal_error::TerminalError;
use super::terminal_events::TerminalEventSink;
use super::terminal_session::{SpawnRequest, TerminalInfo, TerminalSession, TerminalSnapshot};

/// All live terminal sessions. Sessions stay listed after their process exits,
/// so the last output remains readable until the view closes them.
#[derive(Clone)]
pub struct TerminalManager {
    inner: Arc<Inner>,
}

struct Inner {
    shell: Shell,
    sink: Arc<dyn TerminalEventSink>,
    sessions: Mutex<BTreeMap<String, Arc<TerminalSession>>>,
}

impl TerminalManager {
    pub fn new(shell: Shell, sink: Arc<dyn TerminalEventSink>) -> Self {
        Self {
            inner: Arc::new(Inner {
                shell,
                sink,
                sessions: Mutex::new(BTreeMap::new()),
            }),
        }
    }

    pub fn create(
        &self,
        worktree_path: &str,
        command: Option<&str>,
        cols: u16,
        rows: u16,
    ) -> Result<TerminalInfo, TerminalError> {
        let command = command.map(str::trim).filter(|command| !command.is_empty());
        let session = TerminalSession::spawn(
            uuid::Uuid::new_v4().to_string(),
            &SpawnRequest {
                cwd: Path::new(worktree_path),
                shell: &self.inner.shell,
                command,
                cols,
                rows,
            },
            Arc::clone(&self.inner.sink),
        )?;

        let info = session.info();
        self.sessions().insert(info.id.clone(), Arc::new(session));
        Ok(info)
    }

    pub fn list(&self) -> Vec<TerminalInfo> {
        self.sessions()
            .values()
            .map(|session| session.info())
            .collect()
    }

    pub fn snapshot(&self, id: &str) -> Result<TerminalSnapshot, TerminalError> {
        Ok(self.session(id)?.snapshot())
    }

    pub fn write(&self, id: &str, data: &str) -> Result<(), TerminalError> {
        self.session(id)?.write(data)
    }

    pub fn resize(&self, id: &str, cols: u16, rows: u16) -> Result<(), TerminalError> {
        self.session(id)?.resize(cols, rows)
    }

    /// Stops the process if it is still running and forgets the session.
    pub fn close(&self, id: &str) -> Result<(), TerminalError> {
        let session = self
            .sessions()
            .remove(id)
            .ok_or_else(|| TerminalError::NotFound(id.to_owned()))?;
        session.terminate();
        Ok(())
    }

    pub fn close_all(&self) {
        let sessions = std::mem::take(&mut *self.sessions());
        for session in sessions.values() {
            session.terminate();
        }
    }

    fn session(&self, id: &str) -> Result<Arc<TerminalSession>, TerminalError> {
        self.sessions()
            .get(id)
            .cloned()
            .ok_or_else(|| TerminalError::NotFound(id.to_owned()))
    }

    fn sessions(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, Arc<TerminalSession>>> {
        // The map is only mutated by single insert/remove calls, so it is never half-updated.
        self.inner
            .sessions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}
