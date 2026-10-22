//! A terminal's process as the PTY daemon runs it, in the pieces a local PTY gives a session.
//! Each session has its own connection: its messages come in order, and its writes, resizes and
//! kills go out on the same connection.

use std::io::{self, Read, Write};
use std::path::Path;
#[cfg(unix)]
use std::path::PathBuf;
#[cfg(unix)]
use std::sync::mpsc::sync_channel;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use anyhow::Error;
use lynx_browser_protocol::transport::Connection;
use lynx_pty_daemon::client::{connect, receive, write_request};
use lynx_pty_daemon::message::{Message, Request};
use portable_pty::{ChildKiller, ExitStatus, MasterPty, PtySize};

use super::terminal_session::{PtyProcess, SESSION_VARIABLES, SpawnRequest};

/// How long the daemon has to answer a request that starts or joins a session.
const ANSWER_WAIT: Duration = Duration::from_secs(5);
/// How long a question about the foreground waits before it counts as unanswered.
#[cfg(unix)]
const QUERY_WAIT: Duration = Duration::from_secs(1);

/// One session's connection to the daemon, shared by its writer, master and killer.
struct Link {
    id: String,
    writer: Mutex<Connection>,
    /// Where the answer to a foreground question goes, while one is waiting.
    #[cfg(unix)]
    foreground: Mutex<Option<std::sync::mpsc::SyncSender<Option<i32>>>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Link {
    fn send(&self, request: &Request) -> io::Result<()> {
        write_request(&mut *lock(&self.writer), request)
    }

    /// The process group in the foreground of the session's terminal, if the daemon can say.
    #[cfg(unix)]
    fn foreground(&self) -> Option<i32> {
        let (answer, reply) = sync_channel(1);
        *lock(&self.foreground) = Some(answer);
        let group = match self.send(&Request::Foreground {
            id: self.id.clone(),
        }) {
            Ok(()) => reply.recv_timeout(QUERY_WAIT).ok().flatten(),
            Err(_) => None,
        };
        *lock(&self.foreground) = None;
        group
    }
}

/// Starts a session in the daemon in `dir` under `id`, which must be new to it.
pub fn spawn(dir: &Path, id: &str, request: &SpawnRequest<'_>) -> Result<PtyProcess, String> {
    let (mut connection, messages) = open(dir)?;
    let env: Vec<(String, String)> = SESSION_VARIABLES
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .chain(request.env.iter().cloned())
        .collect();
    write_request(
        &mut connection,
        &Request::Create {
            id: id.to_owned(),
            cwd: request.cwd.to_string_lossy().into_owned(),
            program: request.shell.program.clone(),
            args: request.shell.args.clone(),
            env,
            cols: request.cols,
            rows: request.rows,
            outside_app: false,
        },
    )
    .map_err(|error| error.to_string())?;
    match messages.recv_timeout(ANSWER_WAIT) {
        Ok(Message::Created { .. }) => {}
        Ok(Message::Error { message }) => return Err(message),
        Ok(other) => return Err(format!("the daemon answered {other:?}")),
        Err(error) => return Err(format!("the daemon did not answer: {error}")),
    }
    stream(dir, connection, messages, id, request.cols, request.rows)
}

/// Joins a session the daemon already runs under `id`: its output so far, then the live output.
pub fn attach(dir: &Path, id: &str, cols: u16, rows: u16) -> Result<PtyProcess, String> {
    let (connection, messages) = open(dir)?;
    stream(dir, connection, messages, id, cols, rows)
}

fn open(dir: &Path) -> Result<(Connection, Receiver<Message>), String> {
    let connection = connect(dir).map_err(|error| error.to_string())?;
    let messages = receive(connection.clone());
    Ok((connection, messages))
}

/// Asks for the session's output, which comes first, and builds the process around the connection.
fn stream(
    _dir: &Path,
    mut connection: Connection,
    messages: Receiver<Message>,
    id: &str,
    cols: u16,
    rows: u16,
) -> Result<PtyProcess, String> {
    write_request(&mut connection, &Request::Attach { id: id.to_owned() })
        .map_err(|error| error.to_string())?;
    let (first, pid) = loop {
        match messages.recv_timeout(ANSWER_WAIT) {
            Ok(Message::Snapshot { data, pid, .. }) => break (data, pid),
            Ok(Message::Error { message }) => return Err(message),
            Ok(_) => {}
            Err(error) => return Err(format!("the daemon did not answer: {error}")),
        }
    };

    let link = Arc::new(Link {
        id: id.to_owned(),
        writer: Mutex::new(connection),
        #[cfg(unix)]
        foreground: Mutex::new(None),
    });
    let (output_sender, output) = channel();
    let (exit_sender, exit) = channel();
    if !first.is_empty() {
        let _ = output_sender.send(first.into_bytes());
    }
    let routed = Arc::clone(&link);
    std::thread::spawn(move || route(&routed, &messages, &output_sender, &exit_sender));

    Ok(PtyProcess {
        master: Box::new(Master {
            link: Arc::clone(&link),
            size: Mutex::new(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            }),
        }),
        reader: Box::new(Output {
            chunks: output,
            chunk: Vec::new(),
            at: 0,
        }),
        writer: Box::new(Input(Arc::clone(&link))),
        killer: Box::new(Killer(Arc::clone(&link))),
        wait: Box::new(move || match exit.recv() {
            Ok(Some(code)) => Ok(ExitStatus::with_exit_code(u32::try_from(code).unwrap_or(0))),
            _ => Err(io::Error::other(
                "the daemon ended the session without an exit code",
            )),
        }),
        shell_pid: pid,
        daemon_owned: true,
    })
}

/// Sends the session's messages where they belong: output to the reader, the exit code to the
/// waiter, and a foreground answer to the question that asked for it. Ends with the connection.
fn route(
    link: &Link,
    messages: &Receiver<Message>,
    output: &Sender<Vec<u8>>,
    exit: &Sender<Option<i32>>,
) {
    #[cfg(not(unix))]
    let _ = link;
    let mut code = None;
    for message in messages {
        match message {
            Message::Output { data, .. } => {
                if output.send(data.into_bytes()).is_err() {
                    break;
                }
            }
            Message::Exit { code: exited, .. } => {
                code = exited;
                break;
            }
            #[cfg(unix)]
            Message::Foreground { group, .. } => {
                if let Some(answer) = lock(&link.foreground).take() {
                    let _ = answer.send(group);
                }
            }
            _ => {}
        }
    }
    let _ = exit.send(code);
}

/// The session's output as a byte stream. It ends when the daemon ends the session.
struct Output {
    chunks: Receiver<Vec<u8>>,
    chunk: Vec<u8>,
    at: usize,
}

impl Read for Output {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        while self.at == self.chunk.len() {
            self.chunk = match self.chunks.recv() {
                Ok(chunk) => chunk,
                Err(_) => return Ok(0),
            };
            self.at = 0;
        }
        let count = buffer.len().min(self.chunk.len() - self.at);
        buffer[..count].copy_from_slice(&self.chunk[self.at..self.at + count]);
        self.at += count;
        Ok(count)
    }
}

/// Typed input goes to the daemon as text, which is what the terminal sends anyway.
struct Input(Arc<Link>);

impl Write for Input {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let data = std::str::from_utf8(buffer)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        self.0.send(&Request::Write {
            id: self.0.id.clone(),
            data: data.to_owned(),
        })?;
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// The master the session resizes and asks about the foreground through. The daemon owns the
/// streams, so the methods that would hand them out say so.
struct Master {
    link: Arc<Link>,
    size: Mutex<PtySize>,
}

impl MasterPty for Master {
    fn resize(&self, size: PtySize) -> Result<(), Error> {
        self.link.send(&Request::Resize {
            id: self.link.id.clone(),
            cols: size.cols,
            rows: size.rows,
        })?;
        *lock(&self.size) = size;
        Ok(())
    }

    fn get_size(&self) -> Result<PtySize, Error> {
        Ok(*lock(&self.size))
    }

    fn try_clone_reader(&self) -> Result<Box<dyn Read + Send>, Error> {
        Err(Error::msg("the daemon owns the session's output"))
    }

    fn take_writer(&self) -> Result<Box<dyn Write + Send>, Error> {
        Ok(Box::new(Input(Arc::clone(&self.link))))
    }

    #[cfg(unix)]
    fn process_group_leader(&self) -> Option<i32> {
        self.link.foreground()
    }

    #[cfg(unix)]
    fn as_raw_fd(&self) -> Option<std::os::unix::io::RawFd> {
        None
    }

    #[cfg(unix)]
    fn tty_name(&self) -> Option<PathBuf> {
        None
    }
}

/// Ends the session through the daemon, which signals the process groups itself.
struct Killer(Arc<Link>);

impl std::fmt::Debug for Killer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Killer")
    }
}

impl ChildKiller for Killer {
    fn kill(&mut self) -> io::Result<()> {
        self.0.send(&Request::Kill {
            id: self.0.id.clone(),
        })
    }

    fn clone_killer(&self) -> Box<dyn ChildKiller + Send + Sync> {
        Box::new(Killer(Arc::clone(&self.0)))
    }
}
