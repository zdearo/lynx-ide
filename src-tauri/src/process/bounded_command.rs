//! Runs an external CLI for a bounded time: its stdin is written on a thread, its output is read
//! while it runs, and a process still running at the limit is killed.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, ExitStatus, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// How often a streaming run checks whether its command has finished.
const POLL: Duration = Duration::from_millis(50);
/// How long a streaming run waits for the last lines once the command has exited.
const DRAIN: Duration = Duration::from_secs(2);

#[derive(Debug, thiserror::Error)]
pub enum BoundedError {
    #[error(transparent)]
    Spawn(#[from] std::io::Error),
    #[error("the command did not finish in time")]
    TimedOut,
}

/// Runs `command` with `input` on its stdin, and returns its output if it finishes within `timeout`.
pub fn run_bounded(
    command: &mut Command,
    input: &[u8],
    timeout: Duration,
) -> Result<Output, BoundedError> {
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn()?;

    // The writer and readers run on their own threads: the child may answer before it has read
    // everything, and a full output pipe would otherwise block both sides.
    let mut stdin = child.stdin.take();
    let input = input.to_vec();
    let writer = thread::spawn(move || {
        if let Some(stdin) = stdin.as_mut() {
            let _ = stdin.write_all(&input);
        }
    });
    let stdout = read_in_thread(child.stdout.take());
    let stderr = read_in_thread(child.stderr.take());

    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(BoundedError::TimedOut);
        }
        thread::sleep(Duration::from_millis(50));
    };
    let _ = writer.join();
    Ok(Output {
        status,
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
    })
}

/// Runs `command` like `run_bounded`, but calls `on_line` with each line of its output as it arrives,
/// so a reply can be shown while it is still being written. Its stdin is closed at once. Setting
/// `cancel` kills the command at the next check and returns `None`; the lines it wrote until then have
/// been handed over.
pub fn run_streaming(
    command: &mut Command,
    timeout: Duration,
    cancel: &AtomicBool,
    on_line: &mut dyn FnMut(&str),
) -> Result<Option<ExitStatus>, BoundedError> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn()?;
    // Nothing reads stderr here, so it is drained on its own thread, or a full pipe would stop the command.
    let _stderr = read_in_thread(child.stderr.take());

    let (lines_sender, lines) = mpsc::channel::<String>();
    let stdout = child.stdout.take();
    let _reader = thread::spawn(move || {
        if let Some(pipe) = stdout {
            for line in BufReader::new(pipe).lines().map_while(Result::ok) {
                if lines_sender.send(line).is_err() {
                    break;
                }
            }
        }
    });

    let started = Instant::now();
    let status = loop {
        match lines.recv_timeout(POLL) {
            Ok(line) => on_line(&line),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => thread::sleep(POLL),
        }
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if cancel.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            // What it wrote before it was killed still counts.
            while let Ok(line) = lines.try_recv() {
                on_line(&line);
            }
            return Ok(None);
        }
        if started.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(BoundedError::TimedOut);
        }
    };
    while let Ok(line) = lines.recv_timeout(DRAIN) {
        on_line(&line);
    }
    Ok(Some(status))
}

fn read_in_thread<R: Read + Send + 'static>(pipe: Option<R>) -> JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        bytes
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn input_reaches_the_command_and_its_output_comes_back() {
        let mut command = Command::new("/bin/cat");

        let output = run_bounded(&mut command, b"hello", Duration::from_secs(10)).expect("run");

        assert!(output.status.success());
        assert_eq!(output.stdout, b"hello");
    }

    #[test]
    fn each_line_is_handed_over_in_order_as_the_command_writes_it() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "echo one; echo two"]);
        let mut lines = Vec::new();

        let status = run_streaming(
            &mut command,
            Duration::from_secs(10),
            &AtomicBool::new(false),
            &mut |line| lines.push(line.to_owned()),
        )
        .expect("run");

        assert!(status.expect("finished").success());
        assert_eq!(lines, ["one", "two"]);
    }

    #[test]
    fn a_streaming_command_that_runs_too_long_is_stopped() {
        let mut command = Command::new("/bin/sleep");
        command.arg("5");

        let error = run_streaming(
            &mut command,
            Duration::from_millis(200),
            &AtomicBool::new(false),
            &mut |_| {},
        )
        .expect_err("timeout");

        assert!(matches!(error, BoundedError::TimedOut));
    }

    #[test]
    fn a_streaming_command_is_killed_when_it_is_cancelled_and_keeps_what_it_wrote() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "echo first; sleep 30"]);
        let cancel = std::sync::Arc::new(AtomicBool::new(false));
        let flag = std::sync::Arc::clone(&cancel);
        let mut lines = Vec::new();
        let started = Instant::now();

        let status = run_streaming(
            &mut command,
            Duration::from_secs(60),
            &cancel,
            &mut |line| {
                lines.push(line.to_owned());
                flag.store(true, Ordering::SeqCst);
            },
        )
        .expect("run");

        assert!(status.is_none(), "cancelled");
        assert_eq!(lines, ["first"]);
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "it did not wait for the sleep"
        );
    }

    #[test]
    fn a_command_that_runs_too_long_is_stopped() {
        let mut command = Command::new("/bin/sleep");
        command.arg("5");

        let error =
            run_bounded(&mut command, b"", Duration::from_millis(200)).expect_err("timeout");

        assert!(matches!(error, BoundedError::TimedOut));
    }
}
