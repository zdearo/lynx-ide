//! The PTY daemon as the app uses it: starts it when a session needs it, and says whether it is
//! serving. The daemon is this same program run with `--pty-daemon`, so there is one executable to
//! ship, and the AppImage path is the one the hook relay already uses.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use lynx_pty_daemon::client::{receive, write_request};
use lynx_pty_daemon::message::{Message, Request};
use lynx_pty_daemon::server::{DaemonError, Options, is_running, serve};
use serde::Serialize;
use specta::Type;

use super::daemon_pty;
use super::terminal_session::{PtyProcess, SpawnRequest};
use crate::app_image_environment;

/// The argument that makes the program run as the daemon, not as the window.
pub const FLAG: &str = "--pty-daemon";
/// How long a daemon with no sessions stays up. The app ends its sessions when it closes, so this
/// is how long the daemon outlives the app.
const IDLE: Duration = Duration::from_secs(2);
const LAUNCH_WAIT: Duration = Duration::from_secs(3);
const ANSWER_WAIT: Duration = Duration::from_secs(2);
const POLL: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum DaemonState {
    /// No session has asked for the daemon yet.
    Idle,
    Connected,
    /// A session asked for the daemon and could not get it; the daemon is not answering now.
    Degraded,
}

#[derive(Debug, Clone, Copy, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DaemonStatus {
    pub enabled: bool,
    pub state: DaemonState,
    /// Running sessions that the daemon holds.
    pub sessions: u32,
}

pub struct DaemonHost {
    dir: PathBuf,
    executable: Option<PathBuf>,
    /// Read on each use, so a changed setting applies to the next terminal.
    enabled: Arc<dyn Fn() -> bool + Send + Sync>,
    /// Whether the last session that asked for the daemon got it; None before any did.
    serving: Mutex<Option<bool>>,
}

impl DaemonHost {
    pub fn new(
        dir: PathBuf,
        executable: Option<PathBuf>,
        enabled: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Self {
        Self {
            dir,
            executable,
            enabled,
            serving: Mutex::new(None),
        }
    }

    pub fn enabled(&self) -> bool {
        (self.enabled)()
    }

    pub fn state(&self) -> DaemonState {
        match *lock(&self.serving) {
            None => DaemonState::Idle,
            Some(_) if lynx_pty_daemon::client::connect(&self.dir).is_ok() => {
                DaemonState::Connected
            }
            Some(_) => DaemonState::Degraded,
        }
    }

    /// The process for a new session, run by the daemon under `id`. None when the daemon is off or
    /// cannot be reached; the caller then runs the session itself.
    pub fn spawn(&self, id: &str, request: &SpawnRequest<'_>) -> Option<PtyProcess> {
        if !self.enabled() {
            return None;
        }
        let served = daemon_pty::spawn(&self.dir, id, request)
            .or_else(|_| self.launch_and_spawn(id, request));
        *lock(&self.serving) = Some(served.is_ok());
        served.ok()
    }

    /// Starts the daemon and retries until it answers or the wait runs out.
    fn launch_and_spawn(&self, id: &str, request: &SpawnRequest<'_>) -> Result<PtyProcess, String> {
        let executable = self
            .executable
            .as_deref()
            .ok_or_else(|| "no program to start the daemon with".to_owned())?;
        launch(executable, &self.dir)?;
        let deadline = Instant::now() + LAUNCH_WAIT;
        loop {
            match daemon_pty::spawn(&self.dir, id, request) {
                Ok(process) => return Ok(process),
                Err(error) if Instant::now() >= deadline => return Err(error),
                Err(_) => std::thread::sleep(POLL),
            }
        }
    }

    /// Joins a session the daemon already runs. None when no daemon runs it, and nothing is started.
    pub fn attach(&self, id: &str, cols: u16, rows: u16) -> Option<PtyProcess> {
        daemon_pty::attach(&self.dir, id, cols, rows).ok()
    }

    /// The ids of the sessions the running daemon holds, or None when no daemon is running. Never
    /// starts one.
    pub fn live_sessions(&self) -> Option<Vec<String>> {
        let mut connection = lynx_pty_daemon::client::connect(&self.dir).ok()?;
        let messages = receive(connection.clone());
        write_request(&mut connection, &Request::List).ok()?;
        loop {
            match messages.recv_timeout(ANSWER_WAIT) {
                Ok(Message::Sessions { ids }) => return Some(ids),
                Ok(_) => {}
                Err(_) => return None,
            }
        }
    }

    /// Ends the given sessions in the running daemon, if there is one.
    pub fn end_sessions(&self, ids: &[String]) {
        let Ok(mut connection) = lynx_pty_daemon::client::connect(&self.dir) else {
            return;
        };
        for id in ids {
            let _ = write_request(&mut connection, &Request::Kill { id: id.clone() });
        }
    }

    /// Ends every session and waits for the daemon to leave, so the program it runs can be replaced.
    pub fn shut_down(&self) {
        let Ok(mut connection) = lynx_pty_daemon::client::connect(&self.dir) else {
            return;
        };
        let _ = write_request(&mut connection, &Request::Shutdown);
        let deadline = Instant::now() + LAUNCH_WAIT;
        while is_running(&self.dir) && Instant::now() < deadline {
            std::thread::sleep(POLL);
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Starts the daemon detached from the app: it outlives the app's window, and a Ctrl+C in the
/// app does not reach it. Its exit is collected on a thread, so it does not linger as a zombie.
fn launch(executable: &Path, dir: &Path) -> Result<(), String> {
    let mut command = Command::new(daemon_program(executable, dir)?);
    command
        .arg(FLAG)
        .arg(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    detach(&mut command);
    let mut child = command.spawn().map_err(|error| error.to_string())?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// The program the daemon runs. On Windows it is a copy in the app's own folder, because a running
/// program locks its file and the installer must be able to replace the app's. Other platforms run
/// the app's program as it is.
fn daemon_program(executable: &Path, dir: &Path) -> Result<PathBuf, String> {
    #[cfg(windows)]
    {
        let folder = dir.parent().unwrap_or(dir).join("bin");
        let copy = folder.join(format!("lynx-ide-{}.exe", env!("CARGO_PKG_VERSION")));
        if !copy.exists() {
            std::fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
            let partial = copy.with_extension("partial");
            std::fs::copy(executable, &partial).map_err(|error| error.to_string())?;
            std::fs::rename(&partial, &copy).map_err(|error| error.to_string())?;
        }
        Ok(copy)
    }
    #[cfg(not(windows))]
    {
        let _ = dir;
        Ok(executable.to_path_buf())
    }
}

#[cfg(unix)]
fn detach(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(windows)]
fn detach(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    // Unverified here: no Windows machine in this session. Detached from the app's console and
    // process group, so the daemon outlives the app and a Ctrl+C does not reach it.
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
}

/// Runs the daemon when the arguments ask for it and returns the exit code. None means the app
/// should start normally.
pub fn run_if_requested(arguments: &[String]) -> Option<i32> {
    if arguments.get(1).map(String::as_str) != Some(FLAG) {
        return None;
    }
    let Some(dir) = arguments.get(2) else {
        eprintln!("usage: {FLAG} <folder>");
        return Some(2);
    };
    let options = Options {
        idle: IDLE,
        prepare: app_image_environment::clean_terminal,
    };
    Some(match serve(Path::new(dir), &options) {
        Ok(()) | Err(DaemonError::AlreadyRunning) => 0,
        Err(_) => 1,
    })
}
