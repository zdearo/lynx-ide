//! Orchestration: a coordinator agent starts a run, creates tasks, and starts a worker agent for each
//! task in its own worktree. The user approves the run's budget once; the backend keeps to it.

pub mod commands;
pub mod host;
pub mod model;
pub mod router;
pub mod service;
pub mod store;
pub mod tools;
