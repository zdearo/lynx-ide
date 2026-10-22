//! JSON lines in both directions.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Request {
    /// Must be the first request on a connection: the token the daemon wrote in its folder.
    Hello {
        token: String,
    },
    Create {
        /// The id the client gives the session, so it can reattach to it later. Empty means the
        /// daemon chooses one.
        #[serde(default)]
        id: String,
        cwd: String,
        program: String,
        #[serde(default)]
        args: Vec<String>,
        /// Set for the new session's process, on top of the daemon's own environment.
        #[serde(default)]
        env: Vec<(String, String)>,
        cols: u16,
        rows: u16,
    },
    Write {
        id: String,
        data: String,
    },
    Resize {
        id: String,
        cols: u16,
        rows: u16,
    },
    /// Replies with a `Snapshot`, then sends this session's `Output` and `Exit` as they happen.
    Attach {
        id: String,
    },
    Kill {
        id: String,
    },
    /// Asks which process group is in the foreground of the session's terminal (Unix).
    Foreground {
        id: String,
    },
    List,
    /// Ends every session and leaves, so the program the daemon runs can be replaced.
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Message {
    Created {
        id: String,
        /// The shell's process id, when the platform reports one.
        pid: Option<u32>,
    },
    Ok,
    Error {
        message: String,
    },
    Sessions {
        ids: Vec<String>,
    },
    /// What the session printed so far; `last_seq` is the `seq` of its last chunk.
    Snapshot {
        id: String,
        data: String,
        last_seq: u32,
        /// The shell's process id, when the platform reports one.
        pid: Option<u32>,
    },
    Output {
        id: String,
        seq: u32,
        data: String,
    },
    Exit {
        id: String,
        code: Option<i32>,
    },
    Foreground {
        id: String,
        group: Option<i32>,
    },
}
