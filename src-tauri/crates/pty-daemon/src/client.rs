//! The app's side: connects to a running daemon, sends requests, and receives messages on a
//! channel, so a caller can wait for one with a deadline.

use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use lynx_browser_protocol::transport::Connection;

use crate::message::{Message, Request};
use crate::server::ADDRESS_FILE;

pub struct Client {
    connection: Connection,
    messages: Receiver<Message>,
}

impl Client {
    /// Connects to the daemon running in `dir`; an error when none does.
    pub fn connect(dir: &Path) -> io::Result<Self> {
        let address = std::fs::read_to_string(dir.join(ADDRESS_FILE))?;
        let connection = Connection::connect(address.trim())?;
        let (sender, messages) = channel();
        let reading = connection.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(reading).lines() {
                let Ok(line) = line else { return };
                let Ok(message) = serde_json::from_str::<Message>(&line) else {
                    continue;
                };
                if sender.send(message).is_err() {
                    return;
                }
            }
        });
        Ok(Self {
            connection,
            messages,
        })
    }

    pub fn send(&mut self, request: &Request) -> io::Result<()> {
        writeln!(self.connection, "{}", serde_json::to_string(request)?)?;
        self.connection.flush()
    }

    /// The next message, or None when none came in time or the daemon went away.
    pub fn next(&self, within: Duration) -> Option<Message> {
        self.messages.recv_timeout(within).ok()
    }
}
