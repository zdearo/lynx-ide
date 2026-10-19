//! Watches the selected worktree, so the interface reads files again as agents write them.

pub mod change_summary;
pub mod watch_commands;
pub mod watched_directories;
pub mod worktree_events;
pub mod worktree_watcher;
