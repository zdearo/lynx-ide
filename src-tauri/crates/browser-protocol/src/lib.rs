//! The contract between Lynx and `lynx-browser-host`, the process that runs the browser engine.
//!
//! Lynx owns the window and the user; the host owns the engine. They talk over a local socket
//! (JSON lines, see [`message`]) and share frames through a memory-mapped file
//! (see [`frame_buffer`]). [`link`] is the Lynx side: it starts the host and speaks the protocol.

pub mod frame_buffer;
pub mod message;

#[cfg(unix)]
pub mod link;
