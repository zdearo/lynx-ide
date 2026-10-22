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
        /// Started by a client other than the app (a command line): the app shows the session the
        /// next time it opens, and takes over its agent events.
        #[serde(default)]
        outside_app: bool,
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
    /// Stores an agent hook event for a session, for the app to take when it next runs the session.
    /// `payload` is the harness's JSON, as text.
    Event {
        session: String,
        harness: String,
        payload: String,
    },
    /// Takes the events stored for a session, which then no longer wait in the daemon.
    Take {
        session: String,
    },
    /// Lists the sessions started outside the app that are still running and not yet shown.
    Outside,
    /// The app has taken a session started outside it: it is no longer listed.
    Adopt {
        session: String,
    },
    /// Freezes a session's programs, so they stop using the CPU. Unix only for now.
    Hibernate {
        id: String,
    },
    /// Lets a frozen session run again. Writing to it or attaching does this on its own.
    Wake {
        id: String,
    },
    /// Asks what the running sessions cost: memory and CPU time of each one's process tree.
    Usage,
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
    Usage {
        sessions: Vec<SessionUsage>,
    },
    Events {
        events: Vec<PendingEvent>,
    },
    Outside {
        sessions: Vec<OutsideSession>,
    },
}

/// An agent hook event waiting for the app.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingEvent {
    pub harness: String,
    pub payload: String,
}

/// A session a command line started, which the app has not shown yet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutsideSession {
    pub id: String,
    pub cwd: String,
    pub program: String,
}

/// One running session's cost. The tree is the shell and everything it started.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionUsage {
    pub id: String,
    /// Resident memory in KiB. Pages shared between processes count once per process.
    pub memory_kib: u64,
    /// CPU time the tree has used since it started, in milliseconds.
    pub cpu_millis: u64,
}
