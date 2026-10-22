//! The app's side: connects to a running daemon and receives its messages on a channel, so a
//! caller can wait for one with a deadline.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::Path;
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use lynx_browser_protocol::transport::Connection;

use crate::message::{Message, Request};
use crate::server::{ADDRESS_FILE, TOKEN_FILE};

/// The variable a session's process gets with the folder of the daemon that runs it, so an agent's
/// hook can leave its event there when the app is closed.
pub const DIR_ENV: &str = "LYNX_PTY_DAEMON_DIR";

/// Writes one request as a JSON line.
pub fn write_request(writer: &mut impl Write, request: &Request) -> io::Result<()> {
    writeln!(writer, "{}", serde_json::to_string(request)?)?;
    writer.flush()
}

/// The daemon's messages, read on their own thread.
pub fn receive(connection: Connection) -> Receiver<Message> {
    let (sender, messages) = channel();
    std::thread::spawn(move || {
        for line in BufReader::new(connection).lines() {
            let Ok(line) = line else { return };
            let Ok(message) = serde_json::from_str::<Message>(&line) else {
                continue;
            };
            if sender.send(message).is_err() {
                return;
            }
        }
    });
    messages
}

/// Connects to the daemon running in `dir` and proves it may use it. An error when none runs, or
/// when the daemon does not take the token this folder holds.
pub fn connect(dir: &Path) -> io::Result<Connection> {
    let address = std::fs::read_to_string(dir.join(ADDRESS_FILE))?;
    let token = std::fs::read_to_string(dir.join(TOKEN_FILE))?;
    let mut connection = Connection::connect(address.trim())?;
    write_request(
        &mut connection,
        &Request::Hello {
            token: token.trim().to_owned(),
        },
    )?;
    match serde_json::from_str::<Message>(&read_line(&mut connection)?)? {
        Message::Ok => Ok(connection),
        other => Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("the daemon refused this connection: {other:?}"),
        )),
    }
}

/// One line, read a byte at a time so nothing after it is taken. The daemon sends nothing more
/// until it is asked, so no byte is lost by stopping at the line's end.
fn read_line(connection: &mut Connection) -> io::Result<String> {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        if connection.read(&mut byte)? == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        if byte[0] == b'\n' {
            break;
        }
        line.push(byte[0]);
    }
    String::from_utf8(line).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

/// A connection to a daemon, for tests and tools that read everything in order.
pub struct Client {
    writer: Connection,
    messages: Receiver<Message>,
}

impl Client {
    pub fn connect(dir: &Path) -> io::Result<Self> {
        let connection = connect(dir)?;
        Ok(Self {
            messages: receive(connection.clone()),
            writer: connection,
        })
    }

    pub fn send(&mut self, request: &Request) -> io::Result<()> {
        write_request(&mut self.writer, request)
    }

    /// The next message, or None when none came in time or the daemon went away.
    pub fn next(&self, within: Duration) -> Option<Message> {
        self.messages.recv_timeout(within).ok()
    }
}
