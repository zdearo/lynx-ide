//! Running programs for the app: a clean environment for the ones it launches for the user,
//! a time limit for the ones it waits on, and blocking work kept off the async runtime.

pub mod app_image_environment;
pub mod blocking_task;
pub mod bounded_command;
