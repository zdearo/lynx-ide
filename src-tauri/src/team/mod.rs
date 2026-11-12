//! The team: the members of each worktree, who orchestrates whom, and the lines of conversation between
//! them. Its rules are enforced here and in the store, not in the screen.

pub mod team_commands;
pub mod team_error;
pub mod team_events;
pub mod team_knowledge;
pub mod team_model;
pub mod team_rules;
pub mod team_service;
pub mod team_store;

#[cfg(test)]
mod team_service_tests;
