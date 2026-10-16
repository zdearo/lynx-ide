//! The thread that follows one terminal session. A session starts as a plain shell, becomes an
//! agent when a harness reports from it, and goes back to a shell when that agent ends. The
//! monitor owns the title scanner and the activity rules, and tells the sink about each change.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde_json::Value;

use super::activity::{AgentActivity, AgentStatus, HookEvent};
use super::harness::HarnessId;
use super::hook_events;
use super::title_scanner::TitleScanner;
use crate::terminal::TerminalError;
use crate::terminal::terminal_events::TerminalEventSink;

/// How often an agent's monitor wakes with no input, to check the quiet timer.
const TICK: Duration = Duration::from_millis(500);

/// What a session is right now: which agent runs in it, if any, and that agent's status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentSlot {
    pub harness: Option<HarnessId>,
    pub status: AgentStatus,
}

/// What a session's threads send to its monitor, in order.
pub enum AgentInput {
    /// Decoded terminal output. Sent only while the session is an agent.
    Output(String),
    /// A hook report, from the harness the relay was started for.
    Hook { harness: HarnessId, payload: Value },
    /// The user has looked at the session.
    Seen,
}

/// The session's end of its monitor: where input goes, and what the session is right now.
pub struct MonitorLink {
    pub input: Sender<AgentInput>,
    pub slot: Arc<Mutex<AgentSlot>>,
}

/// Starts the monitor for a session. `harness` is the agent it was started with, if any.
/// The monitor ends by itself once every sender is dropped.
pub fn start(
    id: &str,
    harness: Option<HarnessId>,
    sink: Arc<dyn TerminalEventSink>,
) -> Result<MonitorLink, TerminalError> {
    let (input, inbox) = mpsc::channel();
    let slot = Arc::new(Mutex::new(AgentSlot {
        harness,
        status: AgentStatus::Idle,
    }));
    let shared = Arc::clone(&slot);
    let session = id.to_owned();
    std::thread::Builder::new()
        .name(format!("agent-monitor-{session}"))
        .spawn(move || run(&session, &inbox, &shared, sink.as_ref()))
        .map_err(TerminalError::Io)?;
    Ok(MonitorLink { input, slot })
}

fn run(
    id: &str,
    inbox: &Receiver<AgentInput>,
    slot: &Mutex<AgentSlot>,
    sink: &dyn TerminalEventSink,
) {
    let mut monitor = Monitor::new(id, slot, sink);
    loop {
        // A plain shell has nothing to time out, so its monitor just waits for input.
        let received = if monitor.harness().is_some() {
            inbox.recv_timeout(TICK)
        } else {
            inbox.recv().map_err(|_| RecvTimeoutError::Disconnected)
        };
        match received {
            Ok(AgentInput::Output(text)) => monitor.on_output(&text),
            Ok(AgentInput::Hook { harness, payload }) => monitor.on_hook(harness, &payload),
            Ok(AgentInput::Seen) => monitor.on_seen(),
            Err(RecvTimeoutError::Timeout) => monitor.on_tick(Instant::now()),
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

struct Monitor<'a> {
    id: &'a str,
    slot: &'a Mutex<AgentSlot>,
    sink: &'a dyn TerminalEventSink,
    titles: TitleScanner,
    activity: AgentActivity,
}

impl<'a> Monitor<'a> {
    fn new(id: &'a str, slot: &'a Mutex<AgentSlot>, sink: &'a dyn TerminalEventSink) -> Self {
        Self {
            id,
            slot,
            sink,
            titles: TitleScanner::default(),
            activity: AgentActivity::default(),
        }
    }

    fn harness(&self) -> Option<HarnessId> {
        self.slot
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .harness
    }

    fn on_output(&mut self, text: &str) {
        if self.harness().is_none() {
            return;
        }
        let now = Instant::now();
        for title in self.titles.push(text) {
            if let Some(next) = self.activity.on_title(&title, now) {
                self.set_status(next);
            }
        }
        if let Some(next) = self.activity.on_output(now) {
            self.set_status(next);
        }
    }

    fn on_hook(&mut self, harness: HarnessId, payload: &Value) {
        let Some(event) = hook_event(harness, payload) else {
            return;
        };
        match self.harness() {
            // A hook from another harness than the one this session runs is not this session's.
            Some(current) if current != harness => return,
            // A session that ends before it started had no agent to end.
            None if event == HookEvent::Ended => return,
            None => self.become_agent(harness),
            Some(_) => {}
        }
        if event == HookEvent::Ended {
            self.become_shell();
            return;
        }
        if let Some(next) = self.activity.on_hook(event) {
            self.set_status(next);
        }
    }

    fn on_seen(&mut self) {
        if self.harness().is_none() {
            return;
        }
        if let Some(next) = self.activity.mark_seen() {
            self.set_status(next);
        }
    }

    fn on_tick(&mut self, now: Instant) {
        if self.harness().is_none() {
            return;
        }
        if let Some(next) = self.activity.on_tick(now) {
            self.set_status(next);
        }
    }

    /// The session has an agent now, so the rules start fresh for it.
    fn become_agent(&mut self, harness: HarnessId) {
        self.restart_rules();
        self.write(|slot| {
            slot.harness = Some(harness);
            slot.status = AgentStatus::Idle;
        });
    }

    /// The agent has ended, so the session is a shell again.
    fn become_shell(&mut self) {
        self.restart_rules();
        self.write(|slot| {
            slot.harness = None;
            slot.status = AgentStatus::Idle;
        });
    }

    fn restart_rules(&mut self) {
        self.titles = TitleScanner::default();
        self.activity = AgentActivity::default();
    }

    fn set_status(&mut self, next: AgentStatus) {
        self.write(|slot| slot.status = next);
    }

    /// Changes the slot and tells the sink what the session is now.
    fn write(&self, change: impl FnOnce(&mut AgentSlot)) {
        let current = {
            let mut slot = self.slot.lock().unwrap_or_else(PoisonError::into_inner);
            change(&mut slot);
            *slot
        };
        self.sink.agent_state(
            self.id,
            current.harness,
            current.harness.map(|_| current.status),
        );
    }
}

fn hook_event(harness: HarnessId, payload: &Value) -> Option<HookEvent> {
    hook_events::event_for(harness, payload)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex as StdMutex;

    use super::*;

    /// What the sink was told, in order: which agent runs, and its status.
    type Change = (Option<HarnessId>, Option<AgentStatus>);

    #[derive(Default)]
    struct StateSink {
        changes: StdMutex<Vec<Change>>,
    }

    impl TerminalEventSink for StateSink {
        fn output(&self, _id: &str, _seq: u32, _data: String) {}
        fn exit(&self, _id: &str, _exit_code: Option<i32>) {}
        fn agent_state(&self, _id: &str, harness: Option<HarnessId>, status: Option<AgentStatus>) {
            self.changes.lock().expect("lock").push((harness, status));
        }
    }

    fn wait_for_changes(sink: &StateSink, count: usize) -> Vec<Change> {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let changes = sink.changes.lock().expect("lock").clone();
            if changes.len() >= count || Instant::now() > deadline {
                return changes;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn hook(harness: HarnessId, name: &str) -> AgentInput {
        AgentInput::Hook {
            harness,
            payload: serde_json::json!({ "hook_event_name": name, "source": "startup" }),
        }
    }

    #[test]
    fn a_spinner_title_in_the_output_is_reported_as_working() {
        let sink = Arc::new(StateSink::default());
        let link = start(
            "s1",
            Some(HarnessId::Codex),
            Arc::clone(&sink) as Arc<dyn TerminalEventSink>,
        )
        .expect("start");
        link.input
            .send(AgentInput::Output("\x1b]2;⠋ Working\x07".to_owned()))
            .expect("send");

        assert_eq!(
            wait_for_changes(&sink, 1),
            vec![(Some(HarnessId::Codex), Some(AgentStatus::Working))]
        );
        assert_eq!(link.slot.lock().expect("lock").status, AgentStatus::Working);
    }

    #[test]
    fn a_plain_shell_becomes_an_agent_when_a_harness_reports_from_it() {
        let sink = Arc::new(StateSink::default());
        let link =
            start("s1", None, Arc::clone(&sink) as Arc<dyn TerminalEventSink>).expect("start");

        link.input
            .send(hook(HarnessId::Claude, "SessionStart"))
            .expect("send");
        link.input
            .send(AgentInput::Hook {
                harness: HarnessId::Claude,
                payload: serde_json::json!({ "hook_event_name": "UserPromptSubmit" }),
            })
            .expect("send");

        assert_eq!(
            wait_for_changes(&sink, 2),
            vec![
                (Some(HarnessId::Claude), Some(AgentStatus::Idle)),
                (Some(HarnessId::Claude), Some(AgentStatus::Working)),
            ]
        );
    }

    #[test]
    fn session_end_turns_the_agent_back_into_a_shell() {
        let sink = Arc::new(StateSink::default());
        let link =
            start("s1", None, Arc::clone(&sink) as Arc<dyn TerminalEventSink>).expect("start");

        link.input
            .send(hook(HarnessId::Claude, "SessionStart"))
            .expect("send");
        link.input
            .send(hook(HarnessId::Claude, "SessionEnd"))
            .expect("send");

        assert_eq!(
            wait_for_changes(&sink, 2),
            vec![
                (Some(HarnessId::Claude), Some(AgentStatus::Idle)),
                (None, None),
            ]
        );
        assert_eq!(link.slot.lock().expect("lock").harness, None);
    }

    #[test]
    fn a_shell_ignores_terminal_output_and_its_titles() {
        let sink = Arc::new(StateSink::default());
        let link =
            start("s1", None, Arc::clone(&sink) as Arc<dyn TerminalEventSink>).expect("start");

        link.input
            .send(AgentInput::Output("\x1b]2;⠋ Working\x07".to_owned()))
            .expect("send");
        link.input
            .send(hook(HarnessId::Claude, "SessionEnd"))
            .expect("send");

        // Nothing changed for the output; the ended hook on a shell changes nothing either.
        std::thread::sleep(Duration::from_millis(150));
        assert!(sink.changes.lock().expect("lock").is_empty());
        assert_eq!(link.slot.lock().expect("lock").harness, None);
    }

    #[test]
    fn a_hook_from_another_harness_does_not_change_the_session() {
        let sink = Arc::new(StateSink::default());
        let link = start(
            "s1",
            Some(HarnessId::Opencode),
            Arc::clone(&sink) as Arc<dyn TerminalEventSink>,
        )
        .expect("start");

        link.input
            .send(hook(HarnessId::Claude, "UserPromptSubmit"))
            .expect("send");

        std::thread::sleep(Duration::from_millis(150));
        assert!(sink.changes.lock().expect("lock").is_empty());
        assert_eq!(
            link.slot.lock().expect("lock").harness,
            Some(HarnessId::Opencode)
        );
    }

    #[test]
    fn hook_reports_drive_a_claude_session() {
        let sink = Arc::new(StateSink::default());
        let link = start(
            "s1",
            Some(HarnessId::Claude),
            Arc::clone(&sink) as Arc<dyn TerminalEventSink>,
        )
        .expect("start");
        link.input
            .send(AgentInput::Hook {
                harness: HarnessId::Claude,
                payload: serde_json::json!({ "hook_event_name": "UserPromptSubmit" }),
            })
            .expect("send");
        link.input
            .send(AgentInput::Hook {
                harness: HarnessId::Claude,
                payload: serde_json::json!({ "hook_event_name": "Stop" }),
            })
            .expect("send");
        link.input.send(AgentInput::Seen).expect("send");

        assert_eq!(
            wait_for_changes(&sink, 3),
            vec![
                (Some(HarnessId::Claude), Some(AgentStatus::Working)),
                (Some(HarnessId::Claude), Some(AgentStatus::Done)),
                (Some(HarnessId::Claude), Some(AgentStatus::Idle)),
            ]
        );
    }

    #[test]
    fn the_monitor_ends_when_its_last_sender_is_dropped() {
        let sink = Arc::new(StateSink::default());
        let link = start(
            "s1",
            Some(HarnessId::Codex),
            Arc::clone(&sink) as Arc<dyn TerminalEventSink>,
        )
        .expect("start");
        let weak = Arc::downgrade(&link.slot);
        drop(link);
        // The monitor holds the slot until it returns; once it has, only the link did.
        let deadline = Instant::now() + Duration::from_secs(3);
        while weak.strong_count() > 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(weak.strong_count(), 0);
    }
}
