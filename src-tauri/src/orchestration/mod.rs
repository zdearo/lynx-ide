//! Orchestration: an orchestrator in the team starts workers for the tasks of its members. Each worker is the
//! member it was started for, so the team holds the work; this module holds the rules and the tools that act on it.

pub mod agent_wake;
pub mod host;
pub mod router;
pub mod service;
pub mod tools;

#[cfg(test)]
pub(crate) mod service_tests;
