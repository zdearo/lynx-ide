use serde::Serialize;
use specta::Type;

use crate::git::GitError;
use crate::persistence::DatabaseError;
use crate::terminal::TerminalError;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0} is not inside a git repository")]
    NotAGitRepo(String),
    #[error("bare repositories are not supported")]
    BareRepo,
    #[error("repository not found: {0}")]
    RepoNotFound(String),
    #[error("worktree not found: {0}")]
    WorktreeNotFound(String),
    #[error("invalid name: {0}")]
    InvalidName(String),
    #[error("{0} already exists")]
    AlreadyExists(String),
    #[error("the worktree has uncommitted changes")]
    WorktreeHasChanges,
    #[error("the main worktree cannot be removed")]
    CannotRemoveMainWorktree,
    #[error(transparent)]
    Git(#[from] GitError),
    #[error(transparent)]
    Database(#[from] DatabaseError),
    #[error(transparent)]
    Terminal(#[from] TerminalError),
    #[error("background task failed: {0}")]
    TaskFailed(String),
}

/// Error as the frontend sees it: a stable `kind` to branch on and a message to show.
#[derive(Debug, Serialize, Type)]
pub struct IpcError {
    pub kind: IpcErrorKind,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum IpcErrorKind {
    NotAGitRepo,
    BareRepo,
    RepoNotFound,
    WorktreeNotFound,
    InvalidName,
    AlreadyExists,
    WorktreeHasChanges,
    CannotRemoveMainWorktree,
    Git,
    Storage,
    TerminalNotFound,
    TerminalNotRunning,
    Terminal,
    Internal,
}

impl From<AppError> for IpcError {
    fn from(error: AppError) -> Self {
        let kind = match &error {
            AppError::NotAGitRepo(_) => IpcErrorKind::NotAGitRepo,
            AppError::BareRepo => IpcErrorKind::BareRepo,
            AppError::RepoNotFound(_) => IpcErrorKind::RepoNotFound,
            AppError::WorktreeNotFound(_) => IpcErrorKind::WorktreeNotFound,
            AppError::InvalidName(_) => IpcErrorKind::InvalidName,
            AppError::AlreadyExists(_) => IpcErrorKind::AlreadyExists,
            AppError::WorktreeHasChanges => IpcErrorKind::WorktreeHasChanges,
            AppError::CannotRemoveMainWorktree => IpcErrorKind::CannotRemoveMainWorktree,
            AppError::Git(_) => IpcErrorKind::Git,
            AppError::Database(_) => IpcErrorKind::Storage,
            AppError::Terminal(TerminalError::NotFound(_)) => IpcErrorKind::TerminalNotFound,
            AppError::Terminal(TerminalError::NotRunning) => IpcErrorKind::TerminalNotRunning,
            AppError::Terminal(_) => IpcErrorKind::Terminal,
            AppError::TaskFailed(_) => IpcErrorKind::Internal,
        };
        Self {
            kind,
            message: error.to_string(),
        }
    }
}
