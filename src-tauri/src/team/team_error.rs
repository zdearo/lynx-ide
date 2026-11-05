//! Why a team change is refused. A message reaches the screen as it is, so it says what the rule is.

use thiserror::Error;

use crate::persistence::DatabaseError;

#[derive(Debug, Error)]
pub enum TeamError {
    #[error("no member with id {0}")]
    MemberNotFound(String),
    #[error("the role needs 1 to 40 characters")]
    InvalidRole,
    #[error("the task and the done-when line need up to 2000 characters")]
    InvalidText,
    #[error("that is not a bot id")]
    InvalidBot,
    #[error("the worktree needs an absolute path with no .. steps")]
    InvalidWorktree,
    #[error("the session id needs 1 to 128 letters, digits, - or _")]
    InvalidSession,
    #[error("a bot runs no terminal session")]
    BotSession,
    #[error("that bot does not exist")]
    UnknownBot,
    #[error("only an orchestrator can have members under it")]
    NotAnOrchestrator,
    #[error("a member with members under it cannot stop being an orchestrator")]
    HasMembers,
    #[error("a bot only orchestrates, so it stays an orchestrator")]
    BotNotOrchestrator,
    #[error("a bot only orchestrates, so it is never under an orchestrator")]
    BotHasParent,
    #[error("an orchestrator cannot answer to one of its own members")]
    Cycle,
    #[error("a member cannot be its own orchestrator or pair with itself")]
    SameMember,
    #[error("members of different worktrees cannot be linked or answer to each other")]
    OtherWorktree,
    #[error("a bot keeps its name as its role, so only agents take a new role")]
    BotRole,
    #[error("that bot is already in this worktree's team")]
    BotInWorktree,
    #[error("two bots do not talk to each other")]
    BotPair,
    #[error("these two members already talk")]
    AlreadyLinked,
    #[error("an orchestrator and its member already talk through the tree")]
    LinkedToParent,
    #[error("a member with no orchestrator has no task")]
    NoOrchestrator,
    #[error("that terminal already belongs to a member of a team")]
    SessionTaken,
    #[error("a task cannot wait for itself, or for a task that waits for it")]
    DependencyCycle,
    #[error("a task can only wait for tasks of its own worktree")]
    DependencyOtherWorktree,
    #[error("the task already started, so what it waits for cannot change")]
    TaskStarted,
    #[error(transparent)]
    Database(#[from] DatabaseError),
}
