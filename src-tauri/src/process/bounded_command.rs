//! Runs an external CLI for a bounded time: its stdin is written on a thread, its output is read
//! while it runs, and a process still running at the limit is killed.

use std::io::{Read, Write};
use std::process::{Command, Output, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

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
    fn a_command_that_runs_too_long_is_stopped() {
        let mut command = Command::new("/bin/sleep");
        command.arg("5");

        let error =
            run_bounded(&mut command, b"", Duration::from_millis(200)).expect_err("timeout");

        assert!(matches!(error, BoundedError::TimedOut));
    }
}
