mod output_coalescer;
mod scrollback_buffer;
mod shell_resolution;
mod terminal_error;

pub mod daemon_host;
pub mod daemon_pty;
pub mod foreground_job;
pub mod output_channels;
pub mod session_records;
pub mod terminal_commands;
pub mod terminal_events;
pub mod terminal_manager;
pub mod terminal_session;

#[cfg(all(test, unix))]
mod terminal_daemon_tests;
#[cfg(all(test, unix))]
mod terminal_manager_tests;
#[cfg(all(test, unix))]
mod terminal_perf_tests;
#[cfg(test)]
mod terminal_test_support;
#[cfg(all(test, windows))]
mod windows_foreground_tests;
#[cfg(all(test, windows))]
mod windows_terminal_tests;

pub use shell_resolution::Shell;
pub use terminal_error::TerminalError;
