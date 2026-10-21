//! Helpers the PTY tests share, for Unix and Windows alike. Each platform uses only some of them.

#![cfg_attr(not(unix), allow(dead_code))]

use std::fs;
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::agents::activity::AgentStatus;
use crate::agents::harness::HarnessId;
use crate::workspace::linked_folder_sync::without_verbatim_prefix;

use super::terminal_events::TerminalEventSink;

pub(super) const WAIT: Duration = Duration::from_secs(10);

#[derive(Default)]
pub(super) struct Events {
    pub(super) output: Vec<(String, u32, String)>,
    pub(super) exits: Vec<(String, Option<i32>)>,
    /// Agent state reports, in order: session, harness and status.
    pub(super) states: Vec<(String, Option<HarnessId>, Option<AgentStatus>)>,
}

#[derive(Default)]
pub(super) struct RecordingSink {
    pub(super) events: Mutex<Events>,
    changed: Condvar,
}

impl TerminalEventSink for RecordingSink {
    fn output(&self, id: &str, seq: u32, data: String) {
        self.events
            .lock()
            .expect("lock")
            .output
            .push((id.to_owned(), seq, data));
        self.changed.notify_all();
    }

    fn exit(&self, id: &str, exit_code: Option<i32>) {
        self.events
            .lock()
            .expect("lock")
            .exits
            .push((id.to_owned(), exit_code));
        self.changed.notify_all();
    }

    fn agent_state(&self, id: &str, harness: Option<HarnessId>, status: Option<AgentStatus>) {
        self.events
            .lock()
            .expect("lock")
            .states
            .push((id.to_owned(), harness, status));
        self.changed.notify_all();
    }
}

impl RecordingSink {
    pub(super) fn wait_until(&self, what: &str, condition: impl Fn(&Events) -> bool) {
        let deadline = Instant::now() + WAIT;
        let mut events = self.events.lock().expect("lock");
        while !condition(&events) {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(
                !remaining.is_zero(),
                "timed out waiting for {what}; output so far: {:?}",
                events
                    .output
                    .iter()
                    .map(|(_, _, data)| data.as_str())
                    .collect::<String>()
            );
            events = self
                .changed
                .wait_timeout(events, remaining)
                .expect("wait")
                .0;
        }
    }

    pub(super) fn wait_for_text(&self, id: &str, needle: &str) {
        self.wait_until(&format!("{needle:?} in the output"), |events| {
            output_of(events, id).contains(needle)
        });
    }

    /// Waits until the session's latest reported state is this one.
    pub(super) fn wait_for_state(
        &self,
        id: &str,
        harness: Option<HarnessId>,
        status: Option<AgentStatus>,
    ) {
        self.wait_until(&format!("{harness:?} {status:?}"), |events| {
            events
                .states
                .iter()
                .rev()
                .find(|(i, _, _)| i == id)
                .is_some_and(|(_, h, s)| *h == harness && *s == status)
        });
    }

    pub(super) fn wait_for_exit(&self, id: &str) -> Option<i32> {
        self.wait_until("the exit event", |events| {
            events.exits.iter().any(|(i, _)| i == id)
        });
        let events = self.events.lock().expect("lock");
        events
            .exits
            .iter()
            .find(|(i, _)| i == id)
            .and_then(|(_, code)| *code)
    }
}

pub(super) fn output_of(events: &Events, id: &str) -> String {
    events
        .output
        .iter()
        .filter(|(i, _, _)| i == id)
        .map(|(_, _, data)| data.as_str())
        .collect()
}

/// The output without control sequences or whitespace. ConPTY redraws with cursor moves where a
/// program wrote spaces, so a line can arrive in pieces; what is left is what must match.
#[cfg(windows)]
pub(super) fn squeezed(output: &str) -> String {
    let mut text = String::new();
    let mut chars = output.chars();
    while let Some(c) = chars.next() {
        match c {
            '\u{1b}' => match chars.next() {
                // CSI: parameters, then one final byte.
                Some('[') => {
                    for next in chars.by_ref() {
                        if ('\u{40}'..='\u{7e}').contains(&next) {
                            break;
                        }
                    }
                }
                // OSC: up to BEL or ESC \.
                Some(']') => {
                    while let Some(next) = chars.next() {
                        if next == '\u{7}' {
                            break;
                        }
                        if next == '\u{1b}' {
                            chars.next();
                            break;
                        }
                    }
                }
                // Charset selection takes one more character.
                Some('(' | ')') => {
                    chars.next();
                }
                _ => {}
            },
            c if c.is_whitespace() || c.is_control() => {}
            c => text.push(c),
        }
    }
    text
}

/// Waits for text in a session's output, ignoring case, spaces and control sequences.
#[cfg(windows)]
pub(super) fn wait_for_plain_text(sink: &RecordingSink, id: &str, needle: &str) {
    let wanted = squeezed(needle).to_lowercase();
    sink.wait_until(&format!("{needle:?} in the output"), |events| {
        squeezed(&output_of(events, id))
            .to_lowercase()
            .contains(&wanted)
    });
}

/// ConPTY asks where the cursor is and waits for the answer before cmd draws anything. A real
/// terminal (xterm.js) answers that; the tests have none, so they answer as one would.
#[cfg(windows)]
pub(super) fn answer_cursor_request(
    sink: &RecordingSink,
    manager: &super::terminal_manager::TerminalManager,
    id: &str,
) {
    sink.wait_for_text(id, "\u{1b}[6n");
    manager
        .write(id, "\u{1b}[1;1R")
        .expect("answer the cursor request");
}

pub(super) fn path_of(dir: &tempfile::TempDir) -> String {
    // cmd.exe cannot change into a `\\?\` path.
    without_verbatim_prefix(fs::canonicalize(dir.path()).expect("canonical"))
        .to_string_lossy()
        .into_owned()
}
