//! The daemon: owns the sessions, serves any number of connections, and leaves by itself.

use std::collections::{HashMap, VecDeque};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use lynx_browser_protocol::transport::{Connection, Listener};
#[cfg(not(unix))]
use portable_pty::ChildKiller;
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};

use crate::history::History;
use crate::message::{Message, OutsideSession, Request, SessionUsage};
#[cfg(unix)]
#[cfg(unix)]
use crate::process_group::{self, hang_up_and_reap};
use crate::usage;
use crate::utf8_stream_decoder::Utf8StreamDecoder;

/// The lock a running daemon holds. Whoever can take it knows no daemon is alive: the system lets
/// go of it when the holder dies, so a crash leaves nothing stale to guess about.
pub const LOCK_FILE: &str = "daemon.lock";
/// Where a running daemon writes what a client connects to.
pub const ADDRESS_FILE: &str = "address";
/// The secret a client proves it may use the daemon with. Only the owner of the folder can read it.
pub const TOKEN_FILE: &str = "token";
pub const VERSION_FILE: &str = "version";
const SOCKET_FILE: &str = "link.sock";
const SCROLLBACK_BYTES: usize = 1024 * 1024;
const READ_BYTES: usize = 16 * 1024;
const ACCEPT_POLL: Duration = Duration::from_millis(20);
/// Output leaves in batches: a program printing tiny pieces becomes a few messages, not thousands.
const BATCH_INTERVAL: Duration = Duration::from_millis(4);
const BATCH_BYTES: usize = 16 * 1024;
/// Batches a client may fall behind by. Past this it is dropped, so it never holds a session.
const CLIENT_QUEUE: usize = 1024;
/// How long an exit waits for the reader to take the output that was still in the pipe.
const OUTPUT_DRAIN_GRACE: Duration = Duration::from_secs(1);

#[derive(Debug, thiserror::Error)]
pub enum DaemonError {
    #[error("a daemon already runs in this folder")]
    AlreadyRunning,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub struct Options {
    /// How long the daemon stays up with no running session before it leaves.
    pub idle: Duration,
    /// Applied to each session's command before its variables, as the app does for its own shells.
    pub prepare: fn(&mut CommandBuilder),
    /// The app's version, written in the folder so the app can tell whether this daemon is its own.
    pub version: &'static str,
}

/// Whether a daemon holds the lock in `dir`.
pub fn is_running(dir: &Path) -> bool {
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.join(LOCK_FILE))
        .is_ok_and(|file| file.try_lock().is_err())
}

/// Runs the daemon in `dir` until it has been idle for `options.idle`.
pub fn serve(dir: &Path, options: &Options) -> Result<(), DaemonError> {
    fs::create_dir_all(dir)?;
    restrict_to_owner(dir, 0o700)?;
    let _lock = take_lock(dir)?;
    // Holding the lock proves a socket file left here is nobody's, so it is safe to replace.
    let _ = fs::remove_file(dir.join(SOCKET_FILE));
    let token = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    write_private(&dir.join(TOKEN_FILE), &token)?;
    fs::write(dir.join(VERSION_FILE), options.version)?;
    let listener = Listener::bind(dir)?;
    restrict_to_owner(&dir.join(SOCKET_FILE), 0o600)?;
    fs::write(dir.join(ADDRESS_FILE), listener.address())?;
    let outcome = run(&listener, options, token, History::open(dir).ok());
    // Gone before the lock is released, so a client never follows an address nobody answers.
    let _ = fs::remove_file(dir.join(ADDRESS_FILE));
    let _ = fs::remove_file(dir.join(TOKEN_FILE));
    let _ = fs::remove_file(dir.join(VERSION_FILE));
    let _ = fs::remove_file(dir.join(SOCKET_FILE));
    outcome
}

/// Writes through a temporary file that only the owner can read, so the secret is never readable
/// by anyone else, even for a moment.
fn write_private(path: &Path, text: &str) -> io::Result<()> {
    let temporary = path.with_extension("tmp");
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(&temporary)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    drop(file);
    fs::rename(temporary, path)
}

/// Only the owner may enter the folder and touch the socket. Windows keeps the permissions of the
/// folder it is in, and the token is what stops another user there.
#[cfg(unix)]
fn restrict_to_owner(path: &Path, mode: u32) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn restrict_to_owner(_path: &Path, _mode: u32) -> io::Result<()> {
    Ok(())
}

fn run(
    listener: &Listener,
    options: &Options,
    token: String,
    history: Option<History>,
) -> Result<(), DaemonError> {
    let server = Arc::new(Server::new(options.prepare, token, history));
    let mut idle_since: Option<Instant> = None;
    loop {
        if server.stopping.load(std::sync::atomic::Ordering::SeqCst) {
            return Ok(());
        }
        match listener.try_accept()? {
            Some(connection) => {
                let server = Arc::clone(&server);
                std::thread::spawn(move || serve_connection(&server, connection));
            }
            None => std::thread::sleep(ACCEPT_POLL),
        }
        if server.running() > 0 {
            idle_since = None;
        } else if idle_since.get_or_insert_with(Instant::now).elapsed() >= options.idle {
            return Ok(());
        }
    }
}

fn take_lock(dir: &Path) -> Result<File, DaemonError> {
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.join(LOCK_FILE))?;
    file.try_lock().map_err(|error| match error {
        fs::TryLockError::WouldBlock => DaemonError::AlreadyRunning,
        fs::TryLockError::Error(error) => DaemonError::Io(error),
    })?;
    Ok(file)
}

/// Where a connection's messages go. Its own thread writes them, so a client that stops reading
/// only stalls itself.
type Sink = SyncSender<Message>;

/// False when the client has fallen behind or gone; the caller then drops it.
fn send(sink: &Sink, message: Message) -> bool {
    sink.try_send(message).is_ok()
}

fn spawn_writer(connection: Connection) -> Sink {
    let (sender, messages) = sync_channel(CLIENT_QUEUE);
    std::thread::spawn(move || {
        let mut connection = connection;
        for message in messages {
            let Ok(line) = serde_json::to_string(&message) else {
                continue;
            };
            if writeln!(connection, "{line}")
                .and_then(|()| connection.flush())
                .is_err()
            {
                return;
            }
        }
    });
    sender
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // Every critical section here leaves the data consistent, so poisoning is harmless.
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What a session's process is started with.
struct Launch<'a> {
    id: &'a str,
    cwd: &'a str,
    program: &'a str,
    args: &'a [String],
    env: &'a [(String, String)],
    cols: u16,
    rows: u16,
}

struct Server {
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    /// None when the database could not be opened; the sessions still work without it.
    history: Option<Mutex<History>>,
    next_id: AtomicU32,
    prepare: fn(&mut CommandBuilder),
    token: String,
    /// Set by a shutdown request; the serving loop leaves once it sees it.
    stopping: std::sync::atomic::AtomicBool,
}

impl Server {
    fn new(prepare: fn(&mut CommandBuilder), token: String, history: Option<History>) -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            history: history.map(Mutex::new),
            next_id: AtomicU32::new(0),
            prepare,
            token,
            stopping: std::sync::atomic::AtomicBool::new(false),
        }
    }

    fn running(&self) -> usize {
        lock(&self.sessions)
            .values()
            .filter(|session| lock(&session.state).exit.is_none())
            .count()
    }

    fn session(&self, id: &str) -> Result<Arc<Session>, String> {
        lock(&self.sessions)
            .get(id)
            .cloned()
            .ok_or_else(|| format!("no session {id}"))
    }

    fn create(&self, launch: &Launch) -> Result<(String, Option<u32>), String> {
        let id = if launch.id.is_empty() {
            format!("s{}", self.next_id.fetch_add(1, Ordering::Relaxed) + 1)
        } else {
            launch.id.to_owned()
        };
        if lock(&self.sessions).contains_key(&id) {
            return Err(format!("session {id} already exists"));
        }
        let session = Session::spawn(&id, launch, self.prepare)?;
        let pid = session.pid;
        lock(&self.sessions).insert(id.clone(), session);
        Ok((id, pid))
    }

    /// Runs a database call. The database being unavailable is an error for the caller, not a crash.
    fn with_history<T>(
        &self,
        call: impl FnOnce(&History) -> rusqlite::Result<T>,
    ) -> Result<T, String> {
        let history = self
            .history
            .as_ref()
            .ok_or_else(|| "the daemon's database is unavailable".to_owned())?;
        call(&lock(history)).map_err(|error| error.to_string())
    }

    fn record_outside(&self, id: &str, cwd: &str, program: &str) {
        // Best effort: a session that is not recorded is still running, just not shown on open.
        let _ = self.with_history(|history| history.record_outside(id, cwd, program));
    }

    /// The sessions a command line started that still run. Records of ones that ended are dropped.
    fn outside(&self) -> Vec<OutsideSession> {
        let Ok(recorded) = self.with_history(|history| history.outside()) else {
            return Vec::new();
        };
        recorded
            .into_iter()
            .filter_map(|session| {
                let running = self
                    .session(&session.id)
                    .is_ok_and(|found| lock(&found.state).exit.is_none());
                if running {
                    return Some(session);
                }
                let _ = self.with_history(|history| history.forget_outside(&session.id));
                None
            })
            .collect()
    }

    /// What each running session costs, measured now.
    fn usage(&self) -> Vec<SessionUsage> {
        let running: Vec<(String, Option<u32>)> = lock(&self.sessions)
            .values()
            .filter(|session| lock(&session.state).exit.is_none())
            .map(|session| (session.id.clone(), session.pid))
            .collect();
        usage::measure(&running)
    }

    /// Ends every session; the serving loop then leaves.
    fn shut_down(&self) {
        let sessions: Vec<Arc<Session>> = lock(&self.sessions).values().cloned().collect();
        for session in sessions {
            session.kill();
        }
        self.stopping
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// The reply to a request, or None when the connection itself is now a stream of output.
    fn handle(&self, request: Request, sink: &Sink) -> Option<Message> {
        let reply = match request {
            Request::Hello { .. } => Ok(Message::Ok),
            Request::Shutdown => {
                self.shut_down();
                Ok(Message::Ok)
            }
            Request::Create {
                id,
                cwd,
                program,
                args,
                env,
                cols,
                rows,
                outside_app,
            } => self
                .create(&Launch {
                    id: &id,
                    cwd: &cwd,
                    program: &program,
                    args: &args,
                    env: &env,
                    cols,
                    rows,
                })
                .inspect(|(id, _)| {
                    if outside_app {
                        self.record_outside(id, &cwd, &program);
                    }
                })
                .map(|(id, pid)| Message::Created { id, pid }),
            Request::Event {
                session,
                harness,
                payload,
            } => self
                .with_history(|history| history.push_event(&session, &harness, &payload))
                .map(|()| Message::Ok),
            Request::Take { session } => self
                .with_history(|history| history.take_events(&session))
                .map(|events| Message::Events { events }),
            Request::Outside => Ok(Message::Outside {
                sessions: self.outside(),
            }),
            Request::Adopt { session } => self
                .with_history(|history| history.forget_outside(&session))
                .map(|()| Message::Ok),
            Request::Write { id, data } => self
                .session(&id)
                .and_then(|session| session.write(&data))
                .map(|()| Message::Ok),
            Request::Resize { id, cols, rows } => self
                .session(&id)
                .and_then(|session| session.resize(cols, rows))
                .map(|()| Message::Ok),
            Request::Attach { id } => {
                return match self.session(&id) {
                    Ok(session) => {
                        session.attach(sink);
                        None
                    }
                    Err(message) => Some(Message::Error { message }),
                };
            }
            Request::Kill { id } => self.session(&id).map(|session| {
                session.kill();
                lock(&self.sessions).remove(&id);
                Message::Ok
            }),
            Request::Foreground { id } => match self.session(&id) {
                Ok(session) => Ok(Message::Foreground {
                    group: session.foreground(),
                    id,
                }),
                Err(message) => Err(message),
            },
            Request::List => Ok(Message::Sessions {
                ids: lock(&self.sessions).keys().cloned().collect(),
            }),
            Request::Hibernate { id } => self
                .session(&id)
                .and_then(|session| session.hibernate())
                .map(|()| Message::Ok),
            Request::Wake { id } => self.session(&id).map(|session| {
                session.resume_in_shell_then_wait();
                Message::Ok
            }),
            Request::Usage => Ok(Message::Usage {
                sessions: self.usage(),
            }),
        };
        Some(reply.unwrap_or_else(|message| Message::Error { message }))
    }
}

fn serve_connection(server: &Server, connection: Connection) {
    let sink = spawn_writer(connection.clone());
    let mut lines = BufReader::new(connection).lines();
    // Nothing else is answered until the client has shown the token.
    let authorised = matches!(
        lines.next(),
        Some(Ok(line)) if matches!(
            serde_json::from_str::<Request>(&line),
            Ok(Request::Hello { token }) if token == server.token
        )
    );
    if !authorised {
        send(
            &sink,
            Message::Error {
                message: "not allowed: the token is missing or wrong".to_owned(),
            },
        );
        return;
    }
    send(&sink, Message::Ok);
    for line in lines {
        let Ok(line) = line else { return };
        let reply = match serde_json::from_str::<Request>(&line) {
            Ok(request) => server.handle(request, &sink),
            Err(error) => Some(Message::Error {
                message: format!("not a request: {error}"),
            }),
        };
        if let Some(reply) = reply
            && !send(&sink, reply)
        {
            return;
        }
    }
}

struct Session {
    id: String,
    pid: Option<u32>,
    state: Mutex<State>,
    /// Signalled once the reader has taken the last output, so an exit comes after it.
    output_drained: Condvar,
    /// Woken when output is waiting for its batch to be sent.
    flush_due: Condvar,
    writer: Mutex<Box<dyn Write + Send>>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    /// Windows ends a session through its killer; Unix signals the process groups instead.
    #[cfg(not(unix))]
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
}

#[derive(Default)]
struct State {
    chunks: VecDeque<(u32, String)>,
    bytes: usize,
    seq: u32,
    output_done: bool,
    /// Read but not yet sent; it goes out with the next batch.
    pending: String,
    flush_scheduled: bool,
    /// Some once the process ended; the inner value is its exit code.
    exit: Option<Option<i32>>,
    /// The process groups a hibernation froze; empty while the session runs.
    #[cfg(unix)]
    frozen: Vec<i32>,
    /// Whether the shell took the frozen program's terminal back, so it resumes the program with
    /// `fg` rather than a bare signal.
    #[cfg(unix)]
    frozen_through_shell: bool,
    attached: Vec<Sink>,
}

impl State {
    /// Sends what is pending as one batch, and keeps it in the scrollback.
    fn flush(&mut self, id: &str) {
        self.flush_scheduled = false;
        if self.pending.is_empty() {
            return;
        }
        self.seq += 1;
        let seq = self.seq;
        let data = std::mem::take(&mut self.pending);
        self.bytes += data.len();
        self.chunks.push_back((seq, data.clone()));
        while self.bytes > SCROLLBACK_BYTES && self.chunks.len() > 1 {
            if let Some((_, dropped)) = self.chunks.pop_front() {
                self.bytes -= dropped.len();
            }
        }
        self.broadcast(&Message::Output {
            id: id.to_owned(),
            seq,
            data,
        });
    }

    fn broadcast(&mut self, message: &Message) {
        // A client whose queue is full is dropped here, not waited for.
        self.attached
            .retain(|sink| sink.try_send(message.clone()).is_ok());
    }
}

impl Session {
    fn spawn(
        id: &str,
        launch: &Launch,
        prepare: fn(&mut CommandBuilder),
    ) -> Result<Arc<Self>, String> {
        if launch.cols == 0 || launch.rows == 0 {
            return Err("size must be above zero".to_owned());
        }
        if !Path::new(launch.cwd).is_dir() {
            return Err(format!("not a folder: {}", launch.cwd));
        }
        let failed = |error: &dyn std::fmt::Display| error.to_string();
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: launch.rows,
                cols: launch.cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| failed(&e))?;
        let mut command = CommandBuilder::new(launch.program);
        command.args(launch.args);
        command.cwd(launch.cwd);
        prepare(&mut command);
        command.env("TERM", "xterm-256color");
        for (key, value) in launch.env {
            command.env(key, value);
        }
        let mut child = pair.slave.spawn_command(command).map_err(|e| failed(&e))?;
        // Only the child keeps the slave side; otherwise reads never see EOF.
        drop(pair.slave);
        let pid = child.process_id();
        let reader = pair.master.try_clone_reader().map_err(|e| failed(&e))?;
        let writer = pair.master.take_writer().map_err(|e| failed(&e))?;
        #[cfg(not(unix))]
        let killer = child.clone_killer();

        let session = Arc::new(Self {
            id: id.to_owned(),
            pid,
            state: Mutex::new(State::default()),
            output_drained: Condvar::new(),
            flush_due: Condvar::new(),
            writer: Mutex::new(writer),
            master: Mutex::new(pair.master),
            #[cfg(not(unix))]
            killer: Mutex::new(killer),
        });
        let reading = Arc::clone(&session);
        std::thread::spawn(move || reading.read_output(reader));
        let batching = Arc::clone(&session);
        std::thread::spawn(move || batching.flush_batches());
        let waiting = Arc::clone(&session);
        std::thread::spawn(move || {
            let code = child
                .wait()
                .ok()
                .map(|status| i32::try_from(status.exit_code()).unwrap_or(i32::MAX));
            waiting.finish(code);
        });
        Ok(session)
    }

    fn read_output(&self, mut reader: Box<dyn Read + Send>) {
        let mut buffer = [0u8; READ_BYTES];
        // A character can straddle two reads; the decoder holds its bytes back until it is whole.
        let mut decoder = Utf8StreamDecoder::default();
        #[cfg(windows)]
        let mut cursor_answered = false;
        loop {
            let read = match reader.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(read) => read,
            };
            let text = decoder.push(&buffer[..read]);
            if text.is_empty() {
                continue;
            }
            // ConPTY waits for an answer to its cursor request before cmd draws; with nobody
            // attached there is no terminal to give one, so the daemon does.
            #[cfg(windows)]
            if !cursor_answered && text.contains("\u{1b}[6n") {
                cursor_answered = true;
                let _ = self.write("\u{1b}[1;1R");
            }
            let mut state = lock(&self.state);
            state.pending.push_str(&text);
            if state.pending.len() >= BATCH_BYTES {
                state.flush(&self.id);
            } else if !state.flush_scheduled {
                state.flush_scheduled = true;
                self.flush_due.notify_one();
            }
        }
        {
            let mut state = lock(&self.state);
            state.flush(&self.id);
            state.output_done = true;
        }
        self.output_drained.notify_all();
        self.flush_due.notify_all();
    }

    /// Sends the pending output a few milliseconds after it arrives, so a burst becomes one batch.
    fn flush_batches(&self) {
        loop {
            let state = lock(&self.state);
            let state = self
                .flush_due
                .wait_while(state, |state| !state.flush_scheduled && !state.output_done)
                .unwrap_or_else(PoisonError::into_inner);
            if state.output_done {
                return;
            }
            drop(state);
            std::thread::sleep(BATCH_INTERVAL);
            lock(&self.state).flush(&self.id);
        }
    }

    fn finish(&self, code: Option<i32>) {
        let state = lock(&self.state);
        let (mut state, _) = self
            .output_drained
            .wait_timeout_while(state, OUTPUT_DRAIN_GRACE, |state| !state.output_done)
            .unwrap_or_else(PoisonError::into_inner);
        state.exit = Some(code);
        state.broadcast(&Message::Exit {
            id: self.id.clone(),
            code,
        });
    }

    fn attach(&self, sink: &Sink) {
        self.resume_in_shell_then_wait();
        let mut state = lock(&self.state);
        let snapshot = Message::Snapshot {
            id: self.id.clone(),
            data: state.chunks.iter().map(|(_, text)| text.as_str()).collect(),
            last_seq: state.seq,
            pid: self.pid,
        };
        // Under the lock, so no chunk falls between the snapshot and the stream after it. The
        // queue of a new client is empty, so this never waits on the client.
        if !send(sink, snapshot) {
            return;
        }
        if let Some(code) = state.exit {
            send(
                sink,
                Message::Exit {
                    id: self.id.clone(),
                    code,
                },
            );
            return;
        }
        state.attached.push(sink.clone());
    }

    fn write(&self, data: &str) -> Result<(), String> {
        if lock(&self.state).exit.is_some() {
            return Err("the session has ended".to_owned());
        }
        // Typing is the user coming back: a frozen session runs again before it gets the input.
        self.resume_in_shell_then_wait();
        let mut writer = lock(&self.writer);
        writer
            .write_all(data.as_bytes())
            .and_then(|()| writer.flush())
            .map_err(|error| error.to_string())
    }

    fn resize(&self, cols: u16, rows: u16) -> Result<(), String> {
        if cols == 0 || rows == 0 {
            return Err("size must be above zero".to_owned());
        }
        lock(&self.master)
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| error.to_string())
    }

    #[cfg(unix)]
    fn foreground(&self) -> Option<i32> {
        lock(&self.master).process_group_leader()
    }

    #[cfg(not(unix))]
    fn foreground(&self) -> Option<i32> {
        None
    }

    fn kill(&self) {
        if lock(&self.state).exit.is_none() {
            // A frozen program cannot act on a hang-up until it runs again.
            self.thaw(false);
            self.hang_up();
        }
    }

    /// The process groups of the session: the shell's, and the one in the foreground, since
    /// interactive shells give each job its own group.
    #[cfg(unix)]
    fn groups(&self) -> Vec<i32> {
        let mut groups: Vec<i32> = self.foreground().into_iter().collect();
        if let Some(pid) = self.pid.and_then(|pid| i32::try_from(pid).ok()) {
            groups.push(pid);
        }
        groups
    }

    #[cfg(unix)]
    fn hang_up(&self) {
        hang_up_and_reap(self.groups());
    }

    /// Freezes the session's running program, so it uses no CPU. The shell is left running, so it
    /// can resume the program later. A session that is already frozen stays so.
    #[cfg(unix)]
    fn hibernate(&self) -> Result<(), String> {
        let shell = self
            .pid
            .and_then(|pid| i32::try_from(pid).ok())
            .ok_or_else(|| "the shell's process is unknown".to_owned())?;
        let foreground = self.foreground().unwrap_or(shell);
        let mut state = lock(&self.state);
        if state.exit.is_some() {
            return Err("the session has ended".to_owned());
        }
        if state.frozen.is_empty() {
            process_group::stop(&[foreground]);
            state.frozen = vec![foreground];
            // The shell sees the program stop and takes the terminal back, a moment later. Only then
            // can the shell resume the program with `fg`.
            state.frozen_through_shell =
                foreground != shell && self.wait_for_shell_to_take_terminal(shell);
        }
        Ok(())
    }

    #[cfg(not(unix))]
    fn hibernate(&self) -> Result<(), String> {
        Err("hibernating is not available on this platform yet".to_owned())
    }

    /// Lets a frozen session run again, and waits until the program has the terminal, so input sent
    /// next reaches the program and not the shell.
    #[cfg(unix)]
    fn resume_in_shell_then_wait(&self) {
        if let Some(job) = self.thaw(true) {
            let deadline = Instant::now() + Duration::from_secs(1);
            while self.foreground() != Some(job) && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }

    #[cfg(not(unix))]
    fn resume_in_shell_then_wait(&self) {}

    /// Lets a frozen session run again. Through the shell when the shell froze the program, so the
    /// shell gives it the terminal back, and then returns the program's group; a plain resume
    /// otherwise, and when the session is ending.
    #[cfg(unix)]
    fn thaw(&self, through_shell: bool) -> Option<i32> {
        let (frozen, by_shell) = {
            let mut state = lock(&self.state);
            (
                std::mem::take(&mut state.frozen),
                std::mem::take(&mut state.frozen_through_shell),
            )
        };
        if frozen.is_empty() {
            return None;
        }
        if through_shell && by_shell {
            // `fg` is the shell's own job control: it resumes the job and gives it the terminal.
            let mut writer = lock(&self.writer);
            let _ = writer.write_all(b"fg\r").and_then(|()| writer.flush());
            return frozen.first().copied();
        }
        process_group::resume(&frozen);
        None
    }

    #[cfg(not(unix))]
    fn thaw(&self, _through_shell: bool) -> Option<i32> {
        None
    }

    /// Waits, for up to a second, until the shell has the terminal again, which it takes back when
    /// a job stops. Returns whether it did.
    #[cfg(unix)]
    fn wait_for_shell_to_take_terminal(&self, shell: i32) -> bool {
        let deadline = Instant::now() + Duration::from_secs(1);
        while self.foreground() != Some(shell) {
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        true
    }

    #[cfg(not(unix))]
    fn hang_up(&self) {
        let _ = lock(&self.killer).kill();
    }
}

/// Raised when a message changes in a way the other side cannot follow.
pub const PROTOCOL: u32 = 1;

/// The folder a daemon of this protocol version uses under `home`. The version is in the name so
/// a daemon of another version never answers for this one's sessions.
pub fn daemon_dir(home: &Path) -> PathBuf {
    home.join("daemon").join(format!("pty-v{PROTOCOL}"))
}
