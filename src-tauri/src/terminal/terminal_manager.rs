use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use crate::agents::agent_monitor;
use crate::agents::harness::harness_for_command;
use crate::agents::hook_hub::HookHub;
use crate::browser::{PageOwner, SessionOwners};

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

/// Resolved on every new terminal, so a changed setting applies without a restart.
pub type ShellProvider = Arc<dyn Fn() -> Shell + Send + Sync>;

struct Inner {
    shell: ShellProvider,
    sink: Arc<dyn TerminalEventSink>,
    hooks: HookHub,
    owners: SessionOwners,
    sessions: Mutex<BTreeMap<String, Arc<TerminalSession>>>,
}

impl TerminalManager {
    pub fn new(
        shell: ShellProvider,
        sink: Arc<dyn TerminalEventSink>,
        hooks: HookHub,
        owners: SessionOwners,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                shell,
                sink,
                hooks,
                owners,
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
        let shell = (self.inner.shell)();
        let command = command.map(str::trim).filter(|command| !command.is_empty());
        let id = uuid::Uuid::new_v4().to_string();
        // Set before anything below can fail, so each error path must remove it again.
        self.inner.owners.set(
            &id,
            PageOwner::Workspace {
                worktree_path: worktree_path.to_owned(),
            },
        );
        // Every session is followed, so an agent started by hand in a shell is recognised too.
        let monitor = match agent_monitor::start(
            &id,
            command.and_then(harness_for_command),
            Arc::clone(&self.inner.sink),
        ) {
            Ok(monitor) => monitor,
            Err(error) => {
                self.inner.owners.remove(&id);
                return Err(error);
            }
        };
        self.inner.hooks.register(&id, monitor.input.clone());
        let env = self.inner.hooks.session_env(&id);

        let spawned = TerminalSession::spawn(
            id.clone(),
            &SpawnRequest {
                cwd: Path::new(worktree_path),
                shell: &shell,
                command,
                cols,
                rows,
                env: &env,
            },
            Arc::clone(&self.inner.sink),
            monitor,
        );
        let session = match spawned {
            Ok(session) => session,
            Err(error) => {
                self.inner.hooks.unregister(&id);
                self.inner.owners.remove(&id);
                return Err(error);
            }
        };

        let info = session.info();
        self.sessions().insert(info.id.clone(), Arc::new(session));
        Ok(info)
    }

    /// The user looked at an agent session that had finished.
    pub fn mark_seen(&self, id: &str) -> Result<(), TerminalError> {
        self.session(id)?.mark_seen();
        Ok(())
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
        self.inner.hooks.unregister(id);
        self.inner.owners.remove(id);
        session.terminate();
        Ok(())
    }

    pub fn close_all(&self) {
        let sessions = std::mem::take(&mut *self.sessions());
        for (id, session) in &sessions {
            self.inner.hooks.unregister(id);
            self.inner.owners.remove(id);
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
