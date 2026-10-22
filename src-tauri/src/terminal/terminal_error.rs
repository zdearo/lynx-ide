#[derive(Debug, thiserror::Error)]
pub enum TerminalError {
    #[error("terminal not found: {0}")]
    NotFound(String),
    #[error("the terminal process has exited")]
    NotRunning,
    #[error("working directory does not exist: {0}")]
    InvalidWorkingDirectory(String),
    #[error("terminal size must be at least 1x1")]
    InvalidSize,
    #[error("could not start the terminal: {0}")]
    Spawn(String),
    #[error("terminal I/O failed: {0}")]
    Io(#[from] std::io::Error),
    /// The session cannot do what was asked, and the message says why.
    #[error("{0}")]
    Refused(String),
}
