//! The daemon that owns the PTYs. The app asks it for sessions over a local socket and can
//! disconnect and come back: the daemon keeps each session's process and its recent output.
//!
//! This is the first slice: create, write, resize, attach (snapshot, then live output), kill, and
//! leaving on its own once no session runs. Not here yet: binary frames, batching, backpressure,
//! a token, the escape-boundary tracker, history on disk.

pub mod client;
pub mod message;
pub mod server;
