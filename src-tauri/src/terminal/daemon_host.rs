//! The PTY daemon as the app uses it: starts it when a session needs it, and says whether it is
//! serving. The daemon is this same program run with `--pty-daemon`, so there is one executable to
//! ship, and the AppImage path is the one the hook relay already uses.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use lynx_pty_daemon::client::{DIR_ENV, receive, write_request};
use lynx_pty_daemon::message::{Message, OutsideSession, PendingEvent, Request, SessionUsage};
use lynx_pty_daemon::server::{DaemonError, Options, VERSION_FILE, is_running, serve};
use serde::Serialize;
use specta::Type;

use super::daemon_pty;
use super::shell_resolution::Shell;
use super::terminal_session::{PtyProcess, SESSION_VARIABLES, SpawnRequest};
use crate::agents::hook_hub::SESSION_ENV;
use crate::process::app_image_environment::{self, EnvironmentCleanup};

/// The argument that makes the program run as the daemon, not as the window.
pub const FLAG: &str = "--pty-daemon";
/// The argument for a command line that starts a session the app will show when it next opens.
pub const OUTSIDE_FLAG: &str = "--pty-session";
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

    /// Whether the daemon in the folder was started by this version of the app.
    pub fn runs_this_version(&self) -> bool {
        std::fs::read_to_string(self.dir.join(VERSION_FILE))
            .is_ok_and(|version| version == env!("CARGO_PKG_VERSION"))
    }

    /// The folder the daemon runs in; sessions get it, so their agent hooks can find it.
    pub fn dir(&self) -> &Path {
        &self.dir
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

    /// What the running daemon's sessions cost, or None when no daemon answers, or an older one that
    /// does not know the question. Never starts one.
    pub fn usage(&self) -> Option<Vec<SessionUsage>> {
        self.ask(&Request::Usage, |message| match message {
            Message::Usage { sessions } => Some(sessions),
            _ => None,
        })
    }

    /// The sessions a command line started that still run and the app has not shown yet.
    pub fn outside_sessions(&self) -> Option<Vec<OutsideSession>> {
        self.ask(&Request::Outside, |message| match message {
            Message::Outside { sessions } => Some(sessions),
            _ => None,
        })
    }

    /// Tells the daemon the app has taken a session a command line started, so it is not listed.
    pub fn adopt(&self, id: &str) -> bool {
        self.ask(
            &Request::Adopt {
                session: id.to_owned(),
            },
            |message| matches!(message, Message::Ok).then_some(()),
        )
        .is_some()
    }

    /// The agent events the daemon kept for a session while no window was open. Taking them
    /// removes them from the daemon.
    pub fn take_events(&self, id: &str) -> Vec<PendingEvent> {
        self.ask(
            &Request::Take {
                session: id.to_owned(),
            },
            |message| match message {
                Message::Events { events } => Some(events),
                _ => None,
            },
        )
        .unwrap_or_default()
    }

    /// Sends one request on its own connection and returns the first reply `read` accepts. None
    /// when no daemon runs in the folder, or it does not answer in time, or refuses the request.
    fn ask<T>(&self, request: &Request, read: impl Fn(Message) -> Option<T>) -> Option<T> {
        let mut connection = lynx_pty_daemon::client::connect(&self.dir).ok()?;
        let messages = receive(connection.clone());
        write_request(&mut connection, request).ok()?;
        loop {
            match messages.recv_timeout(ANSWER_WAIT) {
                Ok(Message::Error { .. }) | Err(_) => return None,
                Ok(message) => {
                    if let Some(answer) = read(message) {
                        return Some(answer);
                    }
                }
            }
        }
    }

    /// Starts a session for a command line: the daemon keeps it, and the app shows it the next time
    /// it opens. Starts the daemon when none runs. Returns the session's id.
    pub fn start_outside(&self, cwd: &Path, command: Option<&str>) -> Result<String, String> {
        self.ensure_running()?;
        let shell = Shell::from_config(None);
        let id = uuid::Uuid::new_v4().to_string();
        let mut env: Vec<(String, String)> = SESSION_VARIABLES
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect();
        // The agents' hooks find the session and the folder that keeps their events.
        env.push((SESSION_ENV.to_owned(), id.clone()));
        env.push((DIR_ENV.to_owned(), self.dir.to_string_lossy().into_owned()));
        let mut connection =
            lynx_pty_daemon::client::connect(&self.dir).map_err(|error| error.to_string())?;
        let messages = receive(connection.clone());
        write_request(
            &mut connection,
            &Request::Create {
                id: id.clone(),
                cwd: cwd.to_string_lossy().into_owned(),
                program: shell.program,
                args: shell.args,
                env,
                cols: 80,
                rows: 24,
                outside_app: true,
            },
        )
        .map_err(|error| error.to_string())?;
        match messages.recv_timeout(ANSWER_WAIT) {
            Ok(Message::Created { .. }) => {}
            Ok(Message::Error { message }) => return Err(message),
            Ok(other) => return Err(format!("the daemon answered {other:?}")),
            Err(error) => return Err(format!("the daemon did not answer: {error}")),
        }
        if let Some(command) = command {
            write_request(
                &mut connection,
                &Request::Write {
                    id: id.clone(),
                    data: format!("{command}\r"),
                },
            )
            .map_err(|error| error.to_string())?;
        }
        Ok(id)
    }

    /// Sends a request that is answered with Ok. The daemon's refusal comes back as its message.
    pub fn command(&self, request: &Request) -> Result<(), String> {
        let mut connection =
            lynx_pty_daemon::client::connect(&self.dir).map_err(|error| error.to_string())?;
        let messages = receive(connection.clone());
        write_request(&mut connection, request).map_err(|error| error.to_string())?;
        match messages.recv_timeout(ANSWER_WAIT) {
            Ok(Message::Ok) => Ok(()),
            Ok(Message::Error { message }) => Err(message),
            Ok(other) => Err(format!("the daemon answered {other:?}")),
            Err(error) => Err(format!("the daemon did not answer: {error}")),
        }
    }

    /// Starts the daemon when no daemon answers in this folder, and waits until one does.
    fn ensure_running(&self) -> Result<(), String> {
        if lynx_pty_daemon::client::connect(&self.dir).is_ok() {
            return Ok(());
        }
        let executable = self
            .executable
            .as_deref()
            .ok_or_else(|| "no program to start the daemon with".to_owned())?;
        launch(executable, &self.dir)?;
        let deadline = Instant::now() + LAUNCH_WAIT;
        while lynx_pty_daemon::client::connect(&self.dir).is_err() {
            if Instant::now() >= deadline {
                return Err("the daemon did not start".to_owned());
            }
            std::thread::sleep(POLL);
        }
        Ok(())
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
    let mut command = daemon_command(executable, dir, app_image_environment::current())?;
    let mut child = command.spawn().map_err(|error| error.to_string())?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// The command that starts the daemon. It is given the app's environment without the app's AppImage
/// entries: the daemon's own runtime adds its mount, and entries left from the app's mount would
/// reach every terminal the daemon runs, which then loads the app's libraries.
fn daemon_command(
    executable: &Path,
    dir: &Path,
    cleanup: &EnvironmentCleanup,
) -> Result<Command, String> {
    let mut command = Command::new(daemon_program(executable, dir)?);
    command
        .arg(FLAG)
        .arg(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    cleanup.apply_to_command(&mut command);
    detach(&mut command);
    Ok(command)
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

/// Starts a session for a command line when the arguments ask for it, prints its id and returns the
/// exit code. None means the app should start normally.
pub fn start_outside_if_requested(
    arguments: &[String],
    dir: &Path,
    executable: Option<PathBuf>,
) -> Option<i32> {
    if arguments.get(1).map(String::as_str) != Some(OUTSIDE_FLAG) {
        return None;
    }
    let Some(cwd) = arguments.get(2) else {
        eprintln!("usage: {OUTSIDE_FLAG} <folder> [command]");
        return Some(2);
    };
    let command = (arguments.len() > 3).then(|| arguments[3..].join(" "));
    let host = DaemonHost::new(dir.to_owned(), executable, Arc::new(|| true));
    Some(
        match host.start_outside(Path::new(cwd), command.as_deref()) {
            Ok(id) => {
                println!("{id}");
                0
            }
            Err(error) => {
                eprintln!("lynx: {error}");
                1
            }
        },
    )
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
        version: env!("CARGO_PKG_VERSION"),
    };
    Some(match serve(Path::new(dir), &options) {
        Ok(()) | Err(DaemonError::AlreadyRunning) => 0,
        Err(_) => 1,
    })
}

#[cfg(all(test, unix))]
mod tests {
    use std::ffi::OsString;

    use super::*;

    /// The app runs from an AppImage mount and starts the daemon with its own environment. The
    /// daemon must not keep that mount's libraries, or each terminal would load them.
    #[test]
    fn the_daemon_does_not_inherit_the_app_image_mount_of_the_app_that_starts_it() {
        let app = [
            ("APPDIR", "/tmp/.mount_lynx.ahGAhGL"),
            ("APPIMAGE", "/home/me/AppImages/lynx.appimage"),
            (
                "LD_LIBRARY_PATH",
                "/tmp/.mount_lynx.ahGAhGL/usr/lib/:/opt/mine/lib",
            ),
        ]
        .map(|(key, value)| (OsString::from(key), OsString::from(value)));
        let cleanup = EnvironmentCleanup::from_variables(app);

        let command = daemon_command(
            Path::new("/home/me/AppImages/lynx.appimage"),
            Path::new("/home/me/.lynx/daemon/pty-v1"),
            &cleanup,
        )
        .expect("a command");

        let library_path = command
            .get_envs()
            .find(|(key, _)| *key == "LD_LIBRARY_PATH")
            .and_then(|(_, value)| value)
            .map(|value| value.to_string_lossy().into_owned());
        assert_eq!(library_path.as_deref(), Some("/opt/mine/lib"));
        assert!(
            command
                .get_envs()
                .any(|(key, value)| key == "APPDIR" && value.is_none()),
            "the app's mount variable is removed"
        );
    }
}
