//! The pull request of a worktree's branch, read through the user's GitHub CLI (`gh`), which holds
//! the login. Lynx stores no token.

pub mod github_remote;
pub mod pull_request_actions;
pub mod pull_request_commands;
pub mod pull_request_create;
pub mod pull_request_discussion;
pub mod pull_request_draft;
pub mod pull_request_logs;
pub mod pull_request_lookup;
