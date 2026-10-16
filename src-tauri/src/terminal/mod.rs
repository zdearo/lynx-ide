mod scrollback_buffer;
mod shell_resolution;
mod terminal_error;
mod utf8_stream_decoder;

pub mod foreground_job;
pub mod terminal_commands;
pub mod terminal_events;
pub mod terminal_manager;
pub mod terminal_session;

#[cfg(all(test, unix))]
mod terminal_manager_tests;
#[cfg(test)]
mod terminal_test_support;
#[cfg(all(test, windows))]
mod windows_foreground_tests;

pub use shell_resolution::Shell;
pub use terminal_error::TerminalError;
