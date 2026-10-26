use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use lynx_pty_daemon::utf8_stream_decoder::Utf8StreamDecoder;
use portable_pty::{
    ChildKiller, CommandBuilder, ExitStatus, MasterPty, PtySize, native_pty_system,
};
use serde::Serialize;
use specta::Type;

use crate::agents::activity::AgentStatus;
use crate::agents::agent_monitor::{AgentInput, Foreground, ForegroundSource, MonitorLink};
use crate::agents::harness::HarnessId;
use crate::process::app_image_environment;

#[cfg(unix)]
use super::foreground_job::command_line;
use super::output_coalescer::coalesce;
use super::scrollback_buffer::ScrollbackBuffer;
use super::shell_resolution::Shell;
use super::terminal_error::TerminalError;
use super::terminal_events::TerminalEventSink;
#[cfg(unix)]
use lynx_pty_daemon::process_group::hang_up_and_reap;

const READ_BUFFER_BYTES: usize = 16 * 1024;
/// Chunks the reader may get ahead of the thread that sends them on (about 4 MiB).
const READ_QUEUE_CHUNKS: usize = 256;
const SCROLLBACK_BYTES: usize = 1024 * 1024;
/// How long the exit notice waits for the reader to drain the last output.
const OUTPUT_DRAIN_GRACE: Duration = Duration::from_secs(1);

pub struct SpawnRequest<'a> {
    pub cwd: &'a Path,
    pub shell: &'a Shell,
    /// Typed into the shell once it starts, so the tab stays a shell after the program exits.
    pub command: Option<&'a str>,
    pub cols: u16,
    pub rows: u16,
    /// Extra variables for the shell, such as the hook port of an agent session.
    pub env: &'a [(String, String)],
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TerminalInfo {
    pub id: String,
    pub worktree_path: String,
    pub command: Option<String>,
    pub shell: String,
    pub running: bool,
    /// The daemon froze the session to save CPU. Typing into it runs it again.
    pub hibernated: bool,
    pub exit_code: Option<i32>,
    /// Unix seconds when the session started. `u32` because the contract has no 64-bit integers.
    pub started_at: u32,
    /// The agent CLI this session runs; None for a plain shell.
    pub harness: Option<HarnessId>,
    /// The agent's status while it runs; None for a plain shell or an exited session.
    pub agent_status: Option<AgentStatus>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSnapshot {
    pub data: String,
    /// `seq` of the last chunk contained in `data`; 0 when there is no output yet.
    pub last_seq: u32,
    pub running: bool,
    pub exit_code: Option<i32>,
}

pub struct TerminalSession {
    id: String,
    worktree_path: String,
    command: Option<String>,
    shell_name: String,
    started_at: u32,
    shared: Arc<SessionShared>,
    /// Every session has one; it also knows whether the session currently runs an agent.
    monitor: MonitorLink,
    /// Shared with the monitor, which reads the foreground through it.
    pty: Arc<Mutex<PtyHandles>>,
    // Only the Unix hangup reads it; on Windows the foreground is found from it at attach time.
    #[cfg_attr(not(unix), allow(dead_code))]
    shell_pid: Option<u32>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    /// The daemon runs the process: ending it is a request to the daemon, not signals from here.
    daemon_owned: bool,
    /// The daemon froze the session; typing into it runs it again.
    hibernated: AtomicBool,
}

struct PtyHandles {
    writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
}

/// State the reader and waiter threads share with the session.
struct SessionShared {
    output: Mutex<OutputState>,
    exit: Mutex<Option<Option<i32>>>,
    reader_done: Mutex<bool>,
    reader_done_signal: Condvar,
}

struct OutputState {
    scrollback: ScrollbackBuffer,
    seq: u32,
}

impl SessionShared {
    fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
        // Every critical section here leaves the data consistent, so poisoning is harmless.
        mutex.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn mark_reader_done(&self) {
        *Self::lock(&self.reader_done) = true;
        self.reader_done_signal.notify_all();
    }

    fn wait_for_reader(&self, timeout: Duration) {
        let done = Self::lock(&self.reader_done);
        let _ = self
            .reader_done_signal
            .wait_timeout_while(done, timeout, |done| !*done);
    }
}

/// The process a session runs and the pieces it talks through: a local PTY, or the same reached
/// through the PTY daemon.
pub struct PtyProcess {
    pub master: Box<dyn MasterPty + Send>,
    pub reader: Box<dyn Read + Send>,
    pub writer: Box<dyn Write + Send>,
    pub killer: Box<dyn ChildKiller + Send + Sync>,
    pub wait: Box<dyn FnOnce() -> std::io::Result<ExitStatus> + Send>,
    pub shell_pid: Option<u32>,
    pub daemon_owned: bool,
}

/// Variables every session gets; the request's own variables are set after these and win.
pub const SESSION_VARIABLES: [(&str, &str); 2] =
    [("COLORTERM", "truecolor"), ("TERM_PROGRAM", "lynx-ide")];

/// Refuses a request that could not start, before anything is started.
pub fn check_request(request: &SpawnRequest<'_>) -> Result<(), TerminalError> {
    if request.cols == 0 || request.rows == 0 {
        return Err(TerminalError::InvalidSize);
    }
    if !request.cwd.is_dir() {
        return Err(TerminalError::InvalidWorkingDirectory(
            request.cwd.display().to_string(),
        ));
    }
    Ok(())
}

impl TerminalSession {
    pub fn spawn(
        id: String,
        request: &SpawnRequest<'_>,
        sink: Arc<dyn TerminalEventSink>,
        monitor: MonitorLink,
    ) -> Result<Self, TerminalError> {
        check_request(request)?;
        let process = local_pty(request)?;
        Self::start(id, request, sink, monitor, process)
    }

    /// Runs the session on a process that is already started: its command is typed into it, and
    /// its output and exit are followed from here on.
    pub fn start(
        id: String,
        request: &SpawnRequest<'_>,
        sink: Arc<dyn TerminalEventSink>,
        monitor: MonitorLink,
        process: PtyProcess,
    ) -> Result<Self, TerminalError> {
        let PtyProcess {
            master,
            reader,
            mut writer,
            killer,
            wait,
            shell_pid,
            daemon_owned,
        } = process;
        if let Some(command) = request.command {
            // `\r` is what Enter sends, and works for line editors in raw mode too.
            writer.write_all(format!("{command}\r").as_bytes())?;
            writer.flush()?;
        }

        let shared = Arc::new(SessionShared {
            output: Mutex::new(OutputState {
                scrollback: ScrollbackBuffer::new(SCROLLBACK_BYTES),
                seq: 0,
            }),
            exit: Mutex::new(None),
            reader_done: Mutex::new(false),
            reader_done_signal: Condvar::new(),
        });

        let pty = Arc::new(Mutex::new(PtyHandles { writer, master }));
        attach_foreground(&monitor, &pty, shell_pid);
        spawn_reader(
            &id,
            reader,
            Arc::clone(&shared),
            Arc::clone(&sink),
            monitor.input.clone(),
        )?;
        spawn_waiter(&id, wait, Arc::clone(&shared), sink)?;

        Ok(Self {
            id,
            worktree_path: request.cwd.to_string_lossy().into_owned(),
            command: request.command.map(str::to_owned),
            shell_name: request.shell.display_name(),
            started_at: unix_seconds_now(),
            shared,
            monitor,
            pty,
            shell_pid,
            killer: Mutex::new(killer),
            daemon_owned,
            hibernated: AtomicBool::new(false),
        })
    }

    pub fn daemon_owned(&self) -> bool {
        self.daemon_owned
    }

    pub fn hibernated(&self) -> bool {
        self.hibernated.load(Ordering::SeqCst)
    }

    pub fn set_hibernated(&self, value: bool) {
        self.hibernated.store(value, Ordering::SeqCst);
    }

    pub fn info(&self) -> TerminalInfo {
        let exit = *SessionShared::lock(&self.shared.exit);
        let running = exit.is_none();
        let slot = *self
            .monitor
            .slot
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        TerminalInfo {
            id: self.id.clone(),
            worktree_path: self.worktree_path.clone(),
            command: self.command.clone(),
            shell: self.shell_name.clone(),
            running,
            hibernated: running && self.hibernated(),
            exit_code: exit.flatten(),
            started_at: self.started_at,
            harness: slot.harness,
            agent_status: if running && slot.harness.is_some() {
                Some(slot.status)
            } else {
                None
            },
        }
    }

    /// The user has looked at this session.
    pub fn mark_seen(&self) {
        // The monitor ends with the session, so a failed send means nobody is listening.
        let _ = self.monitor.input.send(AgentInput::Seen);
    }

    pub fn snapshot(&self) -> TerminalSnapshot {
        let exit = *SessionShared::lock(&self.shared.exit);
        let output = SessionShared::lock(&self.shared.output);
        TerminalSnapshot {
            data: output.scrollback.contents(),
            last_seq: output.seq,
            running: exit.is_none(),
            exit_code: exit.flatten(),
        }
    }

    pub fn write(&self, data: &str) -> Result<(), TerminalError> {
        if SessionShared::lock(&self.shared.exit).is_some() {
            return Err(TerminalError::NotRunning);
        }
        let mut pty = SessionShared::lock(&self.pty);
        pty.writer.write_all(data.as_bytes())?;
        pty.writer.flush()?;
        Ok(())
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), TerminalError> {
        if cols == 0 || rows == 0 {
            return Err(TerminalError::InvalidSize);
        }
        SessionShared::lock(&self.pty)
            .master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| TerminalError::Io(std::io::Error::other(error.to_string())))
    }

    /// Hangs up the shell and whatever it is running in the foreground, then
    /// kills them if they ignore it. Returns without waiting.
    pub fn terminate(&self) {
        // Once the shell is reaped its pid can be reused, so signalling it would hit a stranger.
        if SessionShared::lock(&self.shared.exit).is_some() {
            return;
        }
        if self.daemon_owned {
            let _ = SessionShared::lock(&self.killer).kill();
        } else {
            self.hang_up_local();
        }
    }

    #[cfg(unix)]
    fn hang_up_local(&self) {
        let mut groups = Vec::new();
        // Interactive shells give each foreground job its own process group.
        if let Some(foreground) = SessionShared::lock(&self.pty).master.process_group_leader() {
            groups.push(foreground);
        }
        if let Some(pid) = self.shell_pid.and_then(|pid| i32::try_from(pid).ok()) {
            groups.push(pid);
        }
        hang_up_and_reap(groups);
    }

    #[cfg(not(unix))]
    fn hang_up_local(&self) {
        let _ = SessionShared::lock(&self.killer).kill();
    }
}

/// A local PTY running the request's shell, as a session holds it.
fn local_pty(request: &SpawnRequest<'_>) -> Result<PtyProcess, TerminalError> {
    let spawn_error = |error: &dyn std::fmt::Display| TerminalError::Spawn(error.to_string());

    let pair = native_pty_system()
        .openpty(PtySize {
            rows: request.rows,
            cols: request.cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| spawn_error(&e))?;

    let mut command = CommandBuilder::new(&request.shell.program);
    command.args(&request.shell.args);
    command.cwd(request.cwd);
    // Before the session's own variables, so an explicit request still wins.
    app_image_environment::clean_terminal(&mut command);
    command.env("TERM", "xterm-256color");
    for (key, value) in SESSION_VARIABLES {
        command.env(key, value);
    }
    for (key, value) in request.env {
        command.env(key, value);
    }

    let mut child = pair
        .slave
        .spawn_command(command)
        .map_err(|e| spawn_error(&e))?;
    // Only the child keeps the slave side; otherwise reads never see EOF.
    drop(pair.slave);

    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| spawn_error(&e))?;
    let writer = pair.master.take_writer().map_err(|e| spawn_error(&e))?;
    Ok(PtyProcess {
        shell_pid: child.process_id(),
        killer: child.clone_killer(),
        master: pair.master,
        reader,
        writer,
        wait: Box::new(move || child.wait()),
        daemon_owned: false,
    })
}

/// Lets the monitor read this terminal's foreground, through the handle the session writes with.
#[cfg(unix)]
fn attach_foreground(monitor: &MonitorLink, pty: &Arc<Mutex<PtyHandles>>, shell_pid: Option<u32>) {
    let Some(shell_pid) = shell_pid.and_then(|pid| i32::try_from(pid).ok()) else {
        return;
    };
    let source = SessionForeground {
        pty: Arc::clone(pty),
        shell_pid,
    };
    let _ = monitor.input.send(AgentInput::Attach(Arc::new(source)));
}

/// Windows has no foreground group, so the foreground is what the shell has started.
#[cfg(windows)]
fn attach_foreground(monitor: &MonitorLink, _pty: &Arc<Mutex<PtyHandles>>, shell_pid: Option<u32>) {
    let Some(shell_pid) = shell_pid else {
        return;
    };
    let _ = monitor
        .input
        .send(AgentInput::Attach(Arc::new(ShellJobs { shell_pid })));
}

#[cfg(not(any(unix, windows)))]
fn attach_foreground(
    _monitor: &MonitorLink,
    _pty: &Arc<Mutex<PtyHandles>>,
    _shell_pid: Option<u32>,
) {
}

/// The foreground on Windows: the job found under the shell in the process table.
#[cfg(windows)]
struct ShellJobs {
    shell_pid: u32,
}

#[cfg(windows)]
impl ForegroundSource for ShellJobs {
    fn foreground(&self) -> Option<Foreground> {
        Some(super::foreground_job::shell_job(self.shell_pid))
    }
}

/// The foreground of a session's terminal: its shell, or the job the shell started.
#[cfg(unix)]
struct SessionForeground {
    pty: Arc<Mutex<PtyHandles>>,
    shell_pid: i32,
}

#[cfg(unix)]
impl ForegroundSource for SessionForeground {
    fn foreground(&self) -> Option<Foreground> {
        let group = SessionShared::lock(&self.pty)
            .master
            .process_group_leader()?;
        if group == self.shell_pid {
            return Some(Foreground::Shell);
        }
        // A job's group is led by the process that started it, so its pid is the group.
        command_line(group).map(Foreground::Job)
    }
}

fn spawn_reader(
    id: &str,
    mut reader: Box<dyn Read + Send>,
    shared: Arc<SessionShared>,
    sink: Arc<dyn TerminalEventSink>,
    input: Sender<AgentInput>,
) -> Result<(), TerminalError> {
    // Bounded, so a view that cannot keep up slows the program down instead of filling memory.
    let (chunks, queue) = std::sync::mpsc::sync_channel::<String>(READ_QUEUE_CHUNKS);
    std::thread::Builder::new()
        .name(format!("pty-reader-{id}"))
        .spawn(move || {
            let mut buffer = [0u8; READ_BUFFER_BYTES];
            let mut decoder = Utf8StreamDecoder::default();
            loop {
                let read = match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(read) => read,
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    // Linux reports the closed slave side as EIO: that is the normal end.
                    Err(_) => break,
                };
                let text = decoder.push(&buffer[..read]);
                // The other side is gone when the send fails, and then nobody needs the output.
                if !text.is_empty() && chunks.send(text).is_err() {
                    break;
                }
            }
        })
        .map_err(TerminalError::Io)?;

    let id = id.to_owned();
    std::thread::Builder::new()
        .name(format!("pty-output-{id}"))
        .spawn(move || {
            coalesce(&queue, |text| {
                // The scrollback and the sequence number change together, so a snapshot
                // always matches the last chunk it claims to contain.
                let seq = {
                    let mut output = SessionShared::lock(&shared.output);
                    output.seq = output.seq.saturating_add(1);
                    output.scrollback.push(text.clone());
                    output.seq
                };
                // The monitor also watches a plain shell, for the line that starts a command.
                // It has ended if the send fails, and then nobody needs the output.
                let _ = input.send(AgentInput::Output(text.clone()));
                sink.output(&id, seq, text);
            });
            shared.mark_reader_done();
        })
        .map(|_| ())
        .map_err(TerminalError::Io)
}

fn spawn_waiter(
    id: &str,
    wait: impl FnOnce() -> std::io::Result<portable_pty::ExitStatus> + Send + 'static,
    shared: Arc<SessionShared>,
    sink: Arc<dyn TerminalEventSink>,
) -> Result<(), TerminalError> {
    let id = id.to_owned();
    std::thread::Builder::new()
        .name(format!("pty-waiter-{id}"))
        .spawn(move || {
            let exit_code = wait()
                .ok()
                .map(|status| i32::try_from(status.exit_code()).unwrap_or(i32::MAX));
            // Let the reader deliver the tail of the output before announcing the exit.
            shared.wait_for_reader(OUTPUT_DRAIN_GRACE);
            *SessionShared::lock(&shared.exit) = Some(exit_code);
            sink.exit(&id, exit_code);
        })
        .map(|_| ())
        .map_err(TerminalError::Io)
}

fn unix_seconds_now() -> u32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u32::try_from(elapsed.as_secs()).unwrap_or(u32::MAX)
        })
}
