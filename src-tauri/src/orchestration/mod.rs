//! Orchestration: a coordinator agent starts a run, creates tasks, and starts a worker agent for each
//! task. Settings say which agent runs by default, where workers work, and whether finished ones close.

pub mod host;
pub mod model;
pub mod router;
pub mod service;
pub mod store;
pub mod tools;

#[cfg(test)]
mod live_run_tests;
#[cfg(test)]
mod service_tests;
