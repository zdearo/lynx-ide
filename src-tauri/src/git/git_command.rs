use std::ffi::{OsStr, OsString};
use std::io;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::process::app_image_environment;
use crate::process::bounded_command::{BoundedError, run_bounded};

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("git executable not found; install git and make sure it is on PATH")]
    NotInstalled,
    #[error("git {args} failed: {stderr}")]
    Failed { args: String, stderr: String },
    #[error("could not run git: {0}")]
    Io(#[source] io::Error),
    #[error("git {args} did not finish in time")]
    TimedOut { args: String },
}

impl GitError {
    pub fn stderr_contains(&self, needle: &str) -> bool {
        matches!(self, GitError::Failed { stderr, .. } if stderr.contains(needle))
    }
}

/// How many git processes a test started in each directory, so a test can say "this takes two".
/// Keyed by directory because tests run side by side, each in a repository of its own.
#[cfg(test)]
pub mod spawn_count {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, OnceLock, PoisonError};

    static COUNTS: OnceLock<Mutex<HashMap<PathBuf, usize>>> = OnceLock::new();

    fn counts() -> std::sync::MutexGuard<'static, HashMap<PathBuf, usize>> {
        COUNTS
            .get_or_init(Mutex::default)
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    pub fn record(cwd: &Path) {
        *counts().entry(cwd.to_path_buf()).or_default() += 1;
    }

    pub fn started_in(cwd: &Path) -> usize {
        counts().get(cwd).copied().unwrap_or(0)
    }
}

/// Runs the user's `git` binary in `cwd`. Output is pinned to the C locale so
/// error messages we match on are stable, and prompts are disabled so a missing
/// credential fails instead of hanging the app.
pub struct GitCommand<'a> {
    cwd: &'a Path,
    args: Vec<OsString>,
    stdin: Option<Vec<u8>>,
    timeout: Option<Duration>,
    literal_paths: bool,
}

pub fn git(cwd: &Path) -> GitCommand<'_> {
    GitCommand {
        cwd,
        args: Vec::new(),
        stdin: None,
        timeout: None,
        literal_paths: false,
    }
}

impl GitCommand<'_> {
    pub fn arg(mut self, arg: impl AsRef<OsStr>) -> Self {
        self.args.push(arg.as_ref().to_owned());
        self
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.args
            .extend(args.into_iter().map(|a| a.as_ref().to_owned()));
        self
    }

    /// Stops git after `timeout`. For commands that wait on the network, which can hang.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Reads path arguments as plain names: a file called `*` or `[a]` is that file, not a
    /// pattern that matches others. For commands that change files; `check-ignore` needs patterns.
    pub fn literal_paths(mut self) -> Self {
        self.literal_paths = true;
        self
    }

    /// Bytes fed to git's standard input.
    pub fn stdin(mut self, input: Vec<u8>) -> Self {
        self.stdin = Some(input);
        self
    }

    fn args_text(&self) -> String {
        self.args
            .iter()
            .map(|a| a.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Returns stdout without the trailing newline.
    pub fn run(self) -> Result<String, GitError> {
        let stdout = self.run_bytes()?;
        Ok(String::from_utf8_lossy(&stdout)
            .trim_end_matches(['\n', '\r'])
            .to_owned())
    }

    /// Returns stdout exactly as git wrote it, for content where every byte counts.
    pub fn run_bytes(self) -> Result<Vec<u8>, GitError> {
        if !self.cwd.is_dir() {
            return Err(GitError::Io(io::Error::new(
                io::ErrorKind::NotFound,
                format!("directory does not exist: {}", self.cwd.display()),
            )));
        }

        #[cfg(test)]
        spawn_count::record(self.cwd);

        let mut command = Command::new("git");
        command
            .current_dir(self.cwd)
            .args(&self.args)
            .env("GIT_TERMINAL_PROMPT", "0")
            // Reading must never rewrite the index: the file watcher would report it and the screen would read again.
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("LC_ALL", "C");
        if self.literal_paths {
            command.env("GIT_LITERAL_PATHSPECS", "1");
        }
        hide_console_window(&mut command);
        app_image_environment::clean_command(&mut command);

        let to_git_error = |error: io::Error| match error.kind() {
            io::ErrorKind::NotFound => GitError::NotInstalled,
            _ => GitError::Io(error),
        };
        let output = match (self.timeout, self.stdin.clone()) {
            (Some(limit), input) => run_bounded(&mut command, &input.unwrap_or_default(), limit)
                .map_err(|error| match error {
                    BoundedError::TimedOut => GitError::TimedOut {
                        args: self.args_text(),
                    },
                    BoundedError::Spawn(source) => to_git_error(source),
                })?,
            (None, None) => command.output().map_err(to_git_error)?,
            (None, Some(input)) => {
                command
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped());
                let mut child = command.spawn().map_err(to_git_error)?;
                let mut stdin = child.stdin.take();
                // Written on its own thread: git may start answering before it has read
                // everything, and a full output pipe would otherwise block us both.
                let writer = std::thread::spawn(move || {
                    if let Some(stdin) = stdin.as_mut() {
                        let _ = stdin.write_all(&input);
                    }
                });
                let output = child.wait_with_output().map_err(to_git_error)?;
                let _ = writer.join();
                output
            }
        };

        if output.status.success() {
            return Ok(output.stdout);
        }

        Err(GitError::Failed {
            args: self.args_text(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        })
    }
}

#[cfg(windows)]
pub(crate) fn hide_console_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
pub(crate) fn hide_console_window(_command: &mut Command) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_with_a_time_limit_still_returns_its_output_and_its_errors() {
        let dir = tempfile::tempdir().expect("temp dir");
        let limit = Duration::from_secs(30);

        let version = git(dir.path())
            .arg("--version")
            .timeout(limit)
            .run()
            .expect("version");
        assert!(version.starts_with("git version"));

        let error = git(dir.path())
            .args(["rev-parse", "--show-toplevel"])
            .timeout(limit)
            .run()
            .expect_err("not a repo");
        assert!(error.stderr_contains("not a git repository"));
    }

    #[test]
    fn failed_command_reports_args_and_stderr() {
        let dir = tempfile::tempdir().expect("temp dir");
        let error = git(dir.path())
            .args(["rev-parse", "--show-toplevel"])
            .run()
            .expect_err("not a repo");
        assert!(error.stderr_contains("not a git repository"));
        assert!(error.to_string().contains("rev-parse --show-toplevel"));
    }

    #[test]
    fn missing_working_directory_is_an_io_error_not_missing_git() {
        let dir = tempfile::tempdir().expect("temp dir");
        let error = git(&dir.path().join("nope")).arg("status").run();
        assert!(matches!(error, Err(GitError::Io(_))));
    }
}
