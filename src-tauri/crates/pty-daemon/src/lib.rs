//! The daemon that owns the PTYs. The app asks it for sessions over a local socket and can
//! disconnect and come back: the daemon keeps each session's process and its recent output.
//!
//! Not here yet: a token, history on disk, reattaching sessions after a daemon restart.

pub mod client;
pub mod history;
pub mod message;
#[cfg(unix)]
pub mod process_group;
pub mod server;
pub mod usage;
pub mod utf8_stream_decoder;
