use std::io;

#[derive(Debug, thiserror::Error)]
pub enum FilesError {
    #[error("not a valid path inside the worktree: {0}")]
    InvalidPath(String),
    #[error("{0} does not exist")]
    NotFound(String),
    #[error("{0} is not a folder")]
    NotADirectory(String),
    #[error("{0} points outside the worktree")]
    OutsideWorktree(String),
    #[error("could not read {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: io::Error,
    },
}
