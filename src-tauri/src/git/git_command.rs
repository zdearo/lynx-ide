use std::ffi::{OsStr, OsString};
use std::io;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("git executable not found; install git and make sure it is on PATH")]
    NotInstalled,
    #[error("git {args} failed: {stderr}")]
    Failed { args: String, stderr: String },
    #[error("could not run git: {0}")]
    Io(#[source] io::Error),
}

impl GitError {
    pub fn stderr_contains(&self, needle: &str) -> bool {
        matches!(self, GitError::Failed { stderr, .. } if stderr.contains(needle))
    }
}

/// Runs the user's `git` binary in `cwd`. Output is pinned to the C locale so
/// error messages we match on are stable, and prompts are disabled so a missing
/// credential fails instead of hanging the app.
pub struct GitCommand<'a> {
    cwd: &'a Path,
    args: Vec<OsString>,
    stdin: Option<Vec<u8>>,
}

pub fn git(cwd: &Path) -> GitCommand<'_> {
    GitCommand {
        cwd,
        args: Vec::new(),
        stdin: None,
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

    /// Bytes fed to git's standard input.
    pub fn stdin(mut self, input: Vec<u8>) -> Self {
        self.stdin = Some(input);
        self
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

        let mut command = Command::new("git");
        command
            .current_dir(self.cwd)
            .args(&self.args)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("LC_ALL", "C");
        hide_console_window(&mut command);

        let to_git_error = |error: io::Error| match error.kind() {
            io::ErrorKind::NotFound => GitError::NotInstalled,
            _ => GitError::Io(error),
        };
        let output = match self.stdin.clone() {
            None => command.output().map_err(to_git_error)?,
            Some(input) => {
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
            args: self
                .args
                .iter()
                .map(|a| a.to_string_lossy())
                .collect::<Vec<_>>()
                .join(" "),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        })
    }
}

#[cfg(windows)]
fn hide_console_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console_window(_command: &mut Command) {}

#[cfg(test)]
mod tests {
    use super::*;

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
