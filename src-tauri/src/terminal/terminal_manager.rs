use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use specta::Type;

use lynx_pty_daemon::message::PendingEvent;

use lynx_pty_daemon::message::Request;

use crate::agents::activity::AgentStatus;
use crate::agents::agent_monitor::{self, AgentInput};
use crate::agents::harness::{
    HarnessId, harness_for_command, harness_name, harness_named, resume_arguments,
};
use crate::agents::hook_hub::HookHub;
use crate::browser::{PageOwner, SessionOwners};
use crate::persistence::{AppDatabase, TerminalSessionRecord};

use super::daemon_host::{DaemonHost, DaemonState, DaemonStatus};
use super::session_records::SessionRecordSink;
use super::shell_resolution::Shell;
use super::terminal_error::TerminalError;
use super::terminal_events::TerminalEventSink;
use super::terminal_session::{
    SpawnRequest, TerminalInfo, TerminalSession, TerminalSnapshot, check_request,
};

/// All live terminal sessions. Sessions stay listed after their process exits,
/// so the last output remains readable until the view closes them.
#[derive(Clone)]
pub struct TerminalManager {
    inner: Arc<Inner>,
}

/// Resolved on every new terminal, so a changed setting applies without a restart.
pub type ShellProvider = Arc<dyn Fn() -> Shell + Send + Sync>;

/// An ended agent session that can be resumed in its worktree.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ResumableSession {
    pub id: String,
    pub harness: HarnessId,
    /// The arguments that resume the conversation, after the agent's own command.
    pub resume_arguments: Vec<String>,
    /// Unix seconds when the session ended. `u32` because the contract has no 64-bit integers.
    pub ended_at: Option<u32>,
}

struct Inner {
    shell: ShellProvider,
    /// Where events go: straight to the screen, or through the database record first.
    sink: Arc<dyn TerminalEventSink>,
    hooks: HookHub,
    owners: SessionOwners,
    /// Runs sessions in the PTY daemon when it is on; sessions run in the app otherwise.
    daemon: Option<DaemonHost>,
    database: Option<Arc<AppDatabase>>,
    /// Whether closing the app leaves the daemon's sessions running for the next start.
    keep_on_exit: AtomicBool,
    sessions: Mutex<BTreeMap<String, Arc<TerminalSession>>>,
}

impl TerminalManager {
    pub fn new(
        shell: ShellProvider,
        sink: Arc<dyn TerminalEventSink>,
        hooks: HookHub,
        owners: SessionOwners,
        daemon: Option<DaemonHost>,
        database: Option<Arc<AppDatabase>>,
    ) -> Self {
        let sink = match &database {
            Some(database) => Arc::new(SessionRecordSink::new(sink, Arc::clone(database)))
                as Arc<dyn TerminalEventSink>,
            None => sink,
        };
        Self {
            inner: Arc::new(Inner {
                shell,
                sink,
                hooks,
                owners,
                daemon,
                database,
                keep_on_exit: AtomicBool::new(false),
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
        let harness = command.and_then(harness_for_command);
        // Recorded before the process starts, so an agent that names its conversation at once finds it.
        self.record_new(&id, worktree_path, harness, command, &shell.program);
        // Set before anything below can fail, so each error path must remove it again.
        self.inner.owners.set(
            &id,
            PageOwner::Workspace {
                worktree_path: worktree_path.to_owned(),
            },
        );
        // Every session is followed, so an agent started by hand in a shell is recognised too.
        let monitor = match agent_monitor::start(&id, harness, Arc::clone(&self.inner.sink)) {
            Ok(monitor) => monitor,
            Err(error) => {
                self.inner.owners.remove(&id);
                self.record_ended(&id);
                return Err(error);
            }
        };
        self.inner.hooks.register(&id, monitor.input.clone());
        let mut env = self.inner.hooks.session_env(&id);
        if let Some(daemon) = &self.inner.daemon {
            // A hook that finds the app closed leaves its event with this folder's daemon.
            env.push((
                lynx_pty_daemon::client::DIR_ENV.to_owned(),
                daemon.dir().to_string_lossy().into_owned(),
            ));
        }

        let request = SpawnRequest {
            cwd: Path::new(worktree_path),
            shell: &shell,
            command,
            cols,
            rows,
            env: &env,
        };
        let daemon_process = self.inner.daemon.as_ref().and_then(|daemon| {
            check_request(&request).ok()?;
            daemon.spawn(&id, &request)
        });
        let spawned = match daemon_process {
            Some(process) => TerminalSession::start(
                id.clone(),
                &request,
                Arc::clone(&self.inner.sink),
                monitor,
                process,
            ),
            None => {
                TerminalSession::spawn(id.clone(), &request, Arc::clone(&self.inner.sink), monitor)
            }
        };
        let session = match spawned {
            Ok(session) => session,
            Err(error) => {
                self.inner.hooks.unregister(&id);
                self.inner.owners.remove(&id);
                self.record_ended(&id);
                return Err(error);
            }
        };

        let info = session.info();
        self.sessions().insert(info.id.clone(), Arc::new(session));
        Ok(info)
    }

    /// Brings back what the last run of the app left: sessions the daemon still runs are joined,
    /// the ones it lost are recorded as ended, and the daemon's sessions the app does not know are
    /// ended. Called once, before the screen opens.
    pub fn restore(&self) {
        self.retire_outdated_daemon();
        let Some(database) = self.inner.database.as_ref() else {
            return;
        };
        let open = database.open_terminal_sessions().unwrap_or_default();
        let live = self
            .inner
            .daemon
            .as_ref()
            .and_then(DaemonHost::live_sessions)
            .unwrap_or_default();
        for record in &open {
            if live.contains(&record.id) {
                if let Err(error) = self.reattach(record) {
                    eprintln!("lynx: could not rejoin session {}: {error}", record.id);
                    self.record_ended(&record.id);
                }
            } else {
                self.record_ended(&record.id);
            }
        }
        let known: Vec<String> = open
            .iter()
            .map(|record| record.id.clone())
            .chain(self.adopt_outside_sessions())
            .collect();
        let unknown: Vec<String> = live.into_iter().filter(|id| !known.contains(id)).collect();
        if let (Some(daemon), false) = (self.inner.daemon.as_ref(), unknown.is_empty()) {
            daemon.end_sessions(&unknown);
        }
    }

    /// A daemon of another version cannot answer the questions this one asks. It keeps its sessions;
    /// once it holds none, it is replaced by one of this version.
    fn retire_outdated_daemon(&self) {
        let Some(daemon) = self.inner.daemon.as_ref() else {
            return;
        };
        if !daemon.runs_this_version() && daemon.live_sessions().is_some_and(|ids| ids.is_empty()) {
            daemon.shut_down();
        }
    }

    /// Shows the sessions a command line started while the app was closed: each gets a record and
    /// is joined like any other. Returns the ids shown.
    fn adopt_outside_sessions(&self) -> Vec<String> {
        let (Some(daemon), Some(database)) =
            (self.inner.daemon.as_ref(), self.inner.database.as_ref())
        else {
            return Vec::new();
        };
        let mut shown = Vec::new();
        for session in daemon.outside_sessions().unwrap_or_default() {
            let record = TerminalSessionRecord {
                id: session.id.clone(),
                worktree_path: session.cwd,
                harness: None,
                command: None,
                shell: session.program,
                resume_id: None,
                started_at: now_seconds(),
                ended_at: None,
                exit_code: None,
            };
            let _ = database.insert_terminal_session(&record);
            match self.reattach(&record) {
                Ok(()) => {
                    daemon.adopt(&record.id);
                    shown.push(record.id);
                }
                Err(error) => {
                    eprintln!("lynx: could not show session {}: {error}", record.id);
                    self.record_ended(&record.id);
                }
            }
        }
        shown
    }

    /// Joins a session the daemon runs, as the app had it, with its agent's state from the record.
    fn reattach(&self, record: &TerminalSessionRecord) -> Result<(), String> {
        let daemon = self.inner.daemon.as_ref().ok_or("the daemon is off")?;
        let process = daemon
            .attach(&record.id, 80, 24)
            .ok_or("the daemon does not run the session")?;
        let harness = record.harness.as_deref().and_then(harness_named);
        let monitor = agent_monitor::start(&record.id, harness, Arc::clone(&self.inner.sink))
            .map_err(|error| error.to_string())?;
        self.inner.hooks.register(&record.id, monitor.input.clone());
        // Events the daemon kept while no window was open reach the agent state first, in order.
        for event in daemon.take_events(&record.id) {
            deliver_stored(&monitor.input, &event);
        }
        self.inner.owners.set(
            &record.id,
            PageOwner::Workspace {
                worktree_path: record.worktree_path.clone(),
            },
        );
        let shell = Shell {
            program: record.shell.clone(),
            args: Vec::new(),
        };
        let request = SpawnRequest {
            cwd: Path::new(&record.worktree_path),
            shell: &shell,
            command: None,
            cols: 80,
            rows: 24,
            env: &[],
        };
        let session = TerminalSession::start(
            record.id.clone(),
            &request,
            Arc::clone(&self.inner.sink),
            monitor,
            process,
        )
        .map_err(|error| error.to_string())?;
        self.sessions().insert(record.id.clone(), Arc::new(session));
        Ok(())
    }

    /// The ended agent sessions of a worktree that can be resumed. A database that cannot be read
    /// lists nothing; the terminals still work.
    pub fn resumable(&self, worktree_path: &str) -> Vec<ResumableSession> {
        let Some(database) = self.inner.database.as_ref() else {
            return Vec::new();
        };
        database
            .resumable_terminal_sessions(worktree_path)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|record| {
                let harness = harness_named(record.harness.as_deref()?)?;
                let resume_id = record.resume_id.as_deref()?;
                Some(ResumableSession {
                    id: record.id,
                    harness,
                    resume_arguments: resume_arguments(harness, resume_id),
                    ended_at: record.ended_at.and_then(|at| u32::try_from(at).ok()),
                })
            })
            .collect()
    }

    /// Takes an ended agent conversation off the resumable list. The conversation stays in the agent.
    pub fn forget_resumable(&self, id: &str) -> Result<(), TerminalError> {
        let database =
            self.inner.database.as_ref().ok_or_else(|| {
                TerminalError::Refused("the session list is unavailable".to_owned())
            })?;
        let forgotten = database
            .forget_resumable_session(id)
            .map_err(|error| TerminalError::Refused(error.to_string()))?;
        if forgotten {
            Ok(())
        } else {
            Err(TerminalError::Refused(
                "only an ended agent session can be removed from the list".to_owned(),
            ))
        }
    }

    /// Chooses what closing the app does with the daemon's sessions: left running, or ended.
    pub fn set_keep_on_exit(&self, keep: bool) {
        self.inner.keep_on_exit.store(keep, Ordering::SeqCst);
    }

    /// Whether the app is closing with sessions the daemon holds, which the user chose to keep.
    pub fn daemon_sessions_running(&self) -> u32 {
        u32::try_from(
            self.sessions()
                .values()
                .filter(|session| session.daemon_owned() && session.info().running)
                .count(),
        )
        .unwrap_or(u32::MAX)
    }

    /// Runs as the app closes. Local sessions end with it. Daemon sessions end too, unless the user
    /// chose to keep them, and then the daemon outlives the app for the next start.
    pub fn quit(&self) {
        let keep = self.inner.keep_on_exit.load(Ordering::SeqCst);
        let sessions: Vec<Arc<TerminalSession>> = self.sessions().values().cloned().collect();
        for session in sessions {
            if keep && session.daemon_owned() {
                continue;
            }
            session.terminate();
        }
        if !keep && let Some(daemon) = self.inner.daemon.as_ref() {
            daemon.shut_down();
        }
    }

    fn record_new(
        &self,
        id: &str,
        worktree_path: &str,
        harness: Option<HarnessId>,
        command: Option<&str>,
        shell: &str,
    ) {
        if let Some(database) = self.inner.database.as_ref() {
            let _ = database.insert_terminal_session(&TerminalSessionRecord {
                id: id.to_owned(),
                worktree_path: worktree_path.to_owned(),
                harness: harness.map(|harness| harness_name(harness).to_owned()),
                command: command.map(str::to_owned),
                shell: shell.to_owned(),
                resume_id: None,
                started_at: now_seconds(),
                ended_at: None,
                exit_code: None,
            });
        }
    }

    fn record_ended(&self, id: &str) {
        if let Some(database) = self.inner.database.as_ref() {
            let _ = database.end_terminal_session(id, None);
        }
    }

    /// The user looked at an agent session that had finished.
    pub fn mark_seen(&self, id: &str) -> Result<(), TerminalError> {
        self.session(id)?.mark_seen();
        Ok(())
    }

    pub fn daemon_status(&self) -> DaemonStatus {
        let sessions = self.daemon_sessions_running();
        match &self.inner.daemon {
            Some(daemon) => DaemonStatus {
                enabled: daemon.enabled(),
                state: daemon.state(),
                sessions,
            },
            None => DaemonStatus {
                enabled: false,
                state: DaemonState::Idle,
                sessions: 0,
            },
        }
    }

    /// Memory the daemon's sessions hold, in MiB. Measured when asked: nothing samples in between.
    pub fn daemon_memory_mib(&self) -> Option<u32> {
        let sessions = self.inner.daemon.as_ref()?.usage()?;
        let kib: u64 = sessions.iter().map(|session| session.memory_kib).sum();
        Some(u32::try_from(kib / 1024).unwrap_or(u32::MAX))
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
        let session = self.session(id)?;
        session.write(data)?;
        // The daemon runs a frozen session again on input, so it is no longer frozen.
        session.set_hibernated(false);
        Ok(())
    }

    /// Freezes a session of the daemon the user is not using, so it takes no CPU. Refused while its
    /// agent works or a browser tool runs for it: a frozen program could not finish that work.
    pub fn hibernate(&self, id: &str) -> Result<(), TerminalError> {
        let session = self.session(id)?;
        if !session.info().running {
            return Err(TerminalError::NotRunning);
        }
        if !session.daemon_owned() {
            return Err(TerminalError::Refused(
                "only a terminal in the background process can sleep".to_owned(),
            ));
        }
        if session.info().agent_status == Some(AgentStatus::Working) {
            return Err(TerminalError::Refused(
                "the agent is working in this terminal".to_owned(),
            ));
        }
        if self.inner.hooks.tool_running(id) {
            return Err(TerminalError::Refused(
                "a browser tool is running for this terminal".to_owned(),
            ));
        }
        let daemon =
            self.inner.daemon.as_ref().ok_or_else(|| {
                TerminalError::Refused("the background process is off".to_owned())
            })?;
        daemon
            .command(&Request::Hibernate { id: id.to_owned() })
            .map_err(TerminalError::Refused)?;
        session.set_hibernated(true);
        Ok(())
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

    #[cfg(test)]
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

/// Hands a stored event to the session's monitor, as the hook hub does for a live one.
fn deliver_stored(input: &Sender<AgentInput>, event: &PendingEvent) {
    let (Some(harness), Ok(payload)) = (
        harness_named(&event.harness),
        serde_json::from_str(&event.payload),
    ) else {
        return;
    };
    let _ = input.send(AgentInput::Hook { harness, payload });
}

fn now_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
        })
}
