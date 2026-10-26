//! What changed in a worktree: the list of changed files and the two sides of one file's diff.

pub mod branch_sync;
pub mod change_discard;
pub mod change_list;
pub mod change_staging;
pub mod changes_commands;
pub mod commit_changes;
pub mod commit_history;
pub mod commit_message_draft;
pub mod commit_message_prompt;
pub mod file_diff;
pub mod line_counts;

#[cfg(test)]
mod changes_perf_tests;
