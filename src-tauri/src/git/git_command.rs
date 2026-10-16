use std::ffi::{OsStr, OsString};
use std::io;
use std::path::Path;
use std::process::Command;

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
}

pub fn git(cwd: &Path) -> GitCommand<'_> {
    GitCommand {
        cwd,
        args: Vec::new(),
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

    /// Returns stdout without the trailing newline.
    pub fn run(self) -> Result<String, GitError> {
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

        let output = command.output().map_err(|error| match error.kind() {
            io::ErrorKind::NotFound => GitError::NotInstalled,
            _ => GitError::Io(error),
        })?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Ok(stdout.trim_end_matches(['\n', '\r']).to_owned());
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
