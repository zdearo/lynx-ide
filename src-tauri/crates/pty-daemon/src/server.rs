//! The daemon: owns the sessions, serves any number of connections, and leaves by itself.

use std::collections::{HashMap, VecDeque};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use lynx_browser_protocol::transport::{Connection, Listener};
use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};

use crate::message::{Message, Request};

/// The lock a running daemon holds. Whoever can take it knows no daemon is alive: the system lets
/// go of it when the holder dies, so a crash leaves nothing stale to guess about.
pub const LOCK_FILE: &str = "daemon.lock";
/// Where a running daemon writes what a client connects to.
pub const ADDRESS_FILE: &str = "address";
const SCROLLBACK_BYTES: usize = 1024 * 1024;
const READ_BYTES: usize = 16 * 1024;
const ACCEPT_POLL: Duration = Duration::from_millis(20);

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
    let _lock = take_lock(dir)?;
    // Holding the lock proves a socket file left here is nobody's, so it is safe to replace.
    #[cfg(unix)]
    let _ = fs::remove_file(dir.join("link.sock"));
    let listener = Listener::bind(dir)?;
    fs::write(dir.join(ADDRESS_FILE), listener.address())?;

    let server = Arc::new(Server::default());
    let mut idle_since: Option<Instant> = None;
    loop {
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

/// One connection's way to receive messages. A reply and a session's output share it.
type Sink = Arc<Mutex<Connection>>;

fn send(sink: &Sink, message: &Message) -> bool {
    let Ok(line) = serde_json::to_string(message) else {
        return false;
    };
    let mut connection = lock(sink);
    writeln!(connection, "{line}").is_ok() && connection.flush().is_ok()
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // Every critical section here leaves the data consistent, so poisoning is harmless.
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Default)]
struct Server {
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    next_id: AtomicU32,
}

impl Server {
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

    fn create(
        &self,
        cwd: &str,
        program: &str,
        args: &[String],
        cols: u16,
        rows: u16,
    ) -> Result<String, String> {
        let id = format!("s{}", self.next_id.fetch_add(1, Ordering::Relaxed) + 1);
        let session = Session::spawn(&id, cwd, program, args, cols, rows)?;
        lock(&self.sessions).insert(id.clone(), session);
        Ok(id)
    }

    fn handle(&self, request: Request, sink: &Sink) -> Option<Message> {
        let reply = match request {
            Request::Create {
                cwd,
                program,
                args,
                cols,
                rows,
            } => self
                .create(&cwd, &program, &args, cols, rows)
                .map(|id| Message::Created { id }),
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
            Request::List => Ok(Message::Sessions {
                ids: lock(&self.sessions).keys().cloned().collect(),
            }),
        };
        Some(reply.unwrap_or_else(|message| Message::Error { message }))
    }
}

fn serve_connection(server: &Server, connection: Connection) {
    let sink: Sink = Arc::new(Mutex::new(connection.clone()));
    for line in BufReader::new(connection).lines() {
        let Ok(line) = line else { return };
        let reply = match serde_json::from_str::<Request>(&line) {
            Ok(request) => server.handle(request, &sink),
            Err(error) => Some(Message::Error {
                message: format!("not a request: {error}"),
            }),
        };
        if let Some(reply) = reply
            && !send(&sink, &reply)
        {
            return;
        }
    }
}

struct Session {
    id: String,
    state: Mutex<State>,
    writer: Mutex<Box<dyn Write + Send>>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
}

#[derive(Default)]
struct State {
    chunks: VecDeque<(u32, String)>,
    bytes: usize,
    seq: u32,
    /// Some once the process ended; the inner value is its exit code.
    exit: Option<Option<i32>>,
    attached: Vec<Sink>,
}

impl State {
    fn broadcast(&mut self, message: &Message) {
        self.attached.retain(|sink| send(sink, message));
    }
}

impl Session {
    fn spawn(
        id: &str,
        cwd: &str,
        program: &str,
        args: &[String],
        cols: u16,
        rows: u16,
    ) -> Result<Arc<Self>, String> {
        if cols == 0 || rows == 0 {
            return Err("size must be above zero".to_owned());
        }
        if !Path::new(cwd).is_dir() {
            return Err(format!("not a folder: {cwd}"));
        }
        let failed = |error: &dyn std::fmt::Display| error.to_string();
        let pair = native_pty_system()
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| failed(&e))?;
        let mut command = CommandBuilder::new(program);
        command.args(args);
        command.cwd(cwd);
        command.env("TERM", "xterm-256color");
        let mut child = pair.slave.spawn_command(command).map_err(|e| failed(&e))?;
        // Only the child keeps the slave side; otherwise reads never see EOF.
        drop(pair.slave);
        let reader = pair.master.try_clone_reader().map_err(|e| failed(&e))?;
        let writer = pair.master.take_writer().map_err(|e| failed(&e))?;

        let session = Arc::new(Self {
            id: id.to_owned(),
            state: Mutex::new(State::default()),
            writer: Mutex::new(writer),
            master: Mutex::new(pair.master),
            killer: Mutex::new(child.clone_killer()),
        });
        let reading = Arc::clone(&session);
        std::thread::spawn(move || reading.read_output(reader));
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

    fn read_output(&self, mut reader: Box<dyn std::io::Read + Send>) {
        let mut buffer = [0u8; READ_BYTES];
        // ConPTY asks where the cursor is and waits; a terminal answers, and with nobody attached
        // the daemon has to.
        #[cfg(windows)]
        let mut cursor_answered = false;
        loop {
            let read = match reader.read(&mut buffer) {
                Ok(0) | Err(_) => return,
                Ok(read) => read,
            };
            // shortcut: a character split across two reads becomes U+FFFD; use a streaming
            // decoder (the app has one) before the app starts reading from the daemon.
            let text = String::from_utf8_lossy(&buffer[..read]).into_owned();
            #[cfg(windows)]
            if !cursor_answered && text.contains("\u{1b}[6n") {
                cursor_answered = true;
                let _ = self.write("\u{1b}[1;1R");
            }
            let mut state = lock(&self.state);
            state.seq += 1;
            let seq = state.seq;
            state.bytes += text.len();
            state.chunks.push_back((seq, text.clone()));
            while state.bytes > SCROLLBACK_BYTES && state.chunks.len() > 1 {
                if let Some((_, dropped)) = state.chunks.pop_front() {
                    state.bytes -= dropped.len();
                }
            }
            state.broadcast(&Message::Output {
                id: self.id.clone(),
                seq,
                data: text,
            });
        }
    }

    fn finish(&self, code: Option<i32>) {
        let mut state = lock(&self.state);
        state.exit = Some(code);
        state.broadcast(&Message::Exit {
            id: self.id.clone(),
            code,
        });
    }

    fn attach(&self, sink: &Sink) {
        let mut state = lock(&self.state);
        let snapshot = Message::Snapshot {
            id: self.id.clone(),
            data: state.chunks.iter().map(|(_, text)| text.as_str()).collect(),
            last_seq: state.seq,
        };
        // Sent under the session's lock, so no chunk can fall between the snapshot and the
        // stream that follows it. shortcut: a client that stops reading holds this lock, and with
        // it the session's output; add a write timeout before more than one client attaches.
        if !send(sink, &snapshot) {
            return;
        }
        if let Some(code) = state.exit {
            send(
                sink,
                &Message::Exit {
                    id: self.id.clone(),
                    code,
                },
            );
            return;
        }
        state.attached.push(Arc::clone(sink));
    }

    fn write(&self, data: &str) -> Result<(), String> {
        if lock(&self.state).exit.is_some() {
            return Err("the session has ended".to_owned());
        }
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

    fn kill(&self) {
        // shortcut: this ends the shell, not what it started; the app's process-group hangup and
        // descendant check move over with the sessions.
        if lock(&self.state).exit.is_none() {
            let _ = lock(&self.killer).kill();
        }
    }
}

/// Raised when a message changes in a way the other side cannot follow.
pub const PROTOCOL: u32 = 1;

/// The folder a daemon of this protocol version uses under `home`. The version is in the name so
/// a daemon of another version never answers for this one's sessions.
pub fn daemon_dir(home: &Path) -> PathBuf {
    home.join("daemon").join(format!("pty-v{PROTOCOL}"))
}
