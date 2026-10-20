//! Talks to a provider's own CLI over newline-delimited JSON on stdio, the way its clients do.
//! Lines are read on their own thread, so a deadline still applies to a CLI that stops answering.

use std::io::{self, BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::Instant;

use serde_json::Value;

use crate::agents::agent_catalog::on_path;
use crate::git::git_command::hide_console_window;

/// How a provider's CLI is started. The program is looked up on PATH, so tests can use another one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderCommand {
    program: String,
    args: Vec<String>,
}

impl ProviderCommand {
    pub fn new(program: &str, args: &[&str]) -> Self {
        Self {
            program: program.to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        }
    }

    pub fn is_installed(&self) -> bool {
        on_path(&self.program)
    }

    /// Starts the CLI with its input and output piped. A Windows CLI is usually a `.cmd` shim, which only the shell can start.
    pub fn spawn(&self) -> io::Result<Child> {
        let mut process = if cfg!(windows) {
            let mut process = Command::new("cmd");
            process.args(["/C", &self.program]);
            process
        } else {
            Command::new(&self.program)
        };
        process.args(&self.args);
        hide_console_window(&mut process);
        process
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
    }
}

/// Why a wait for an answer ended without one.
#[derive(Debug, PartialEq, Eq)]
pub enum Silence {
    TimedOut,
    Closed,
}

impl Silence {
    pub fn describe(&self, provider: &str) -> String {
        match self {
            Self::TimedOut => format!("{provider} did not answer in time"),
            Self::Closed => format!("{provider} closed the connection"),
        }
    }
}

pub fn take_input(child: &mut Child) -> Result<ChildStdin, String> {
    child
        .stdin
        .take()
        .ok_or_else(|| "the CLI gave no input channel".to_owned())
}

/// Lines the CLI writes to stdout. The channel closes when the CLI does.
pub fn lines_of(child: &mut Child) -> Result<Receiver<String>, String> {
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "the CLI gave no output channel".to_owned())?;
    let (sender, lines) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    Ok(lines)
}

pub fn send(stdin: &mut ChildStdin, message: &Value) -> Result<(), String> {
    writeln!(stdin, "{message}")
        .and_then(|()| stdin.flush())
        .map_err(|error| format!("could not write to the CLI: {error}"))
}

/// Waits for the first message that `is_answer` accepts. Lines that are not JSON, or not theirs, are skipped.
pub fn wait_for(
    lines: &Receiver<String>,
    deadline: Instant,
    is_answer: impl Fn(&Value) -> bool,
) -> Result<Value, Silence> {
    loop {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or(Silence::TimedOut)?;
        let line = lines.recv_timeout(remaining).map_err(|error| match error {
            RecvTimeoutError::Timeout => Silence::TimedOut,
            RecvTimeoutError::Disconnected => Silence::Closed,
        })?;
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if is_answer(&message) {
            return Ok(message);
        }
    }
}

/// The CLI has nothing left to do once it has answered, and must not outlive the read.
pub fn stop(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}
