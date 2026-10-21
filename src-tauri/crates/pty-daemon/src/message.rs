//! JSON lines in both directions.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Request {
    Create {
        cwd: String,
        program: String,
        args: Vec<String>,
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
    List,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Message {
    Created {
        id: String,
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
}
