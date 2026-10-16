//! The thread that follows one terminal session. A session starts as a plain shell and becomes an
//! agent when a harness reports from it, or when its foreground job is one. It goes back to a shell
//! when that agent ends. The monitor owns the title scanner and the activity rules, and tells the
//! sink about each change.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde_json::Value;

use super::activity::{AgentActivity, AgentStatus, HookEvent};
use super::harness::{HarnessId, harness_of_job};
use super::hook_events;
use super::title_scanner::TitleScanner;
use crate::terminal::TerminalError;
use crate::terminal::terminal_events::TerminalEventSink;

/// How often the monitor wakes while it has something to watch: an agent's quiet timer, or a look
/// at the foreground.
const TICK: Duration = Duration::from_millis(250);
/// The shortest time between two looks at the foreground job.
const LOOK_SPACING: Duration = Duration::from_millis(500);
/// After a line of output the foreground is looked at for this long. A command has just been
/// submitted, and the job it starts may take a moment to appear.
const LAUNCH_WINDOW: Duration = Duration::from_secs(3);

/// What a session is right now: which agent runs in it, if any, and that agent's status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentSlot {
    pub harness: Option<HarnessId>,
    pub status: AgentStatus,
}

/// What runs in the foreground of a terminal: the shell itself, or a job started from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Foreground {
    Shell,
    /// The job's arguments, the program first.
    Job(Vec<String>),
}

/// Looks at what is in the foreground of one terminal. None when that cannot be told.
pub trait ForegroundSource: Send + Sync {
    fn foreground(&self) -> Option<Foreground>;
}

/// What a session's threads send to its monitor, in order.
pub enum AgentInput {
    /// Decoded terminal output, from any session: a line of it means a command was submitted.
    Output(String),
    /// A hook report, from the harness the relay was started for.
    Hook { harness: HarnessId, payload: Value },
    /// The user has looked at the session.
    Seen,
    /// Where the foreground of the session can be read. Sent once, when the shell starts.
    Attach(Arc<dyn ForegroundSource>),
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
        // A plain shell with nothing to look at has nothing to time out, so it just waits for input.
        let received = if monitor.wants_tick() {
            inbox.recv_timeout(TICK)
        } else {
            inbox.recv().map_err(|_| RecvTimeoutError::Disconnected)
        };
        match received {
            Ok(AgentInput::Output(text)) => monitor.on_output(&text),
            Ok(AgentInput::Hook { harness, payload }) => monitor.on_hook(harness, &payload),
            Ok(AgentInput::Seen) => monitor.on_seen(),
            Ok(AgentInput::Attach(source)) => monitor.foreground = Some(source),
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
    foreground: Option<Arc<dyn ForegroundSource>>,
    /// Whether the agent's job has been seen in the foreground. Until then the foreground does not
    /// end the agent: a hook says the agent runs, but not that its job is in front of the user.
    seen: bool,
    /// Until when the foreground is looked at on every tick, after a line of output.
    launch_until: Option<Instant>,
    /// Output arrived since the last look, and an agent may have changed what it runs.
    look_pending: bool,
    last_look: Option<Instant>,
}

impl<'a> Monitor<'a> {
    fn new(id: &'a str, slot: &'a Mutex<AgentSlot>, sink: &'a dyn TerminalEventSink) -> Self {
        Self {
            id,
            slot,
            sink,
            titles: TitleScanner::default(),
            activity: AgentActivity::default(),
            foreground: None,
            seen: false,
            launch_until: None,
            look_pending: false,
            last_look: None,
        }
    }

    fn harness(&self) -> Option<HarnessId> {
        self.slot
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .harness
    }

    /// Whether the monitor must wake on its own: an agent's timers run, or a look is due.
    fn wants_tick(&self) -> bool {
        self.harness().is_some() || self.launch_until.is_some()
    }

    fn on_output(&mut self, text: &str) {
        let now = Instant::now();
        if let Some(harness) = self.harness() {
            for title in self.titles.push(text) {
                if let Some(next) = self.activity.on_title(harness, &title, now) {
                    self.set_status(next);
                }
            }
            if let Some(next) = self.activity.on_output(now) {
                self.set_status(next);
            }
            self.look_pending = true;
        }
        if text.contains('\n') {
            self.launch_until = Some(now + LAUNCH_WINDOW);
        }
        self.maybe_look(now);
    }

    fn on_hook(&mut self, harness: HarnessId, payload: &Value) {
        let Some(event) = hook_events::event_for(harness, payload) else {
            return;
        };
        match self.harness() {
            // A hook from another harness than the one this session runs is not this session's.
            Some(current) if current != harness => return,
            // A session that ends before it started had no agent to end.
            None if event == HookEvent::Ended => return,
            // Grok also runs the hooks of Claude Code, so a Claude hook can arrive from a Grok job.
            // The job in the foreground says whose the hook is.
            None if self.foreground_runs_another(harness) => return,
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
        if self.harness().is_some()
            && let Some(next) = self.activity.on_tick(now)
        {
            self.set_status(next);
        }
        if self.launch_until.is_some_and(|until| now >= until) {
            self.launch_until = None;
        }
        self.maybe_look(now);
    }

    /// Looks at the foreground when output or the launch window asks for it, at most every
    /// `LOOK_SPACING`. A look that was held back stays pending for the next tick.
    fn maybe_look(&mut self, now: Instant) {
        let launching = self.launch_until.is_some_and(|until| now < until);
        if !(self.look_pending || launching) {
            return;
        }
        let spaced = self
            .last_look
            .is_none_or(|last| now.duration_since(last) >= LOOK_SPACING);
        let Some(source) = self.foreground.clone() else {
            self.look_pending = false;
            return;
        };
        if !spaced {
            return;
        }
        self.look_pending = false;
        self.last_look = Some(now);
        self.on_foreground(source.foreground());
    }

    fn on_foreground(&mut self, foreground: Option<Foreground>) {
        match (self.harness(), foreground) {
            (None, Some(Foreground::Job(argv))) => {
                if let Some(harness) = harness_of_job(&argv) {
                    self.launch_until = None;
                    self.become_agent(harness);
                    self.seen = true;
                }
            }
            (Some(current), Some(Foreground::Job(argv))) => {
                if harness_of_job(&argv) == Some(current) {
                    self.seen = true;
                }
            }
            // The agent's job has left the foreground: the agent is over. Not before it was seen,
            // because a command typed a moment ago may not have started yet.
            (Some(_), Some(Foreground::Shell)) => {
                if self.seen {
                    self.become_shell();
                }
            }
            (None, Some(Foreground::Shell)) | (_, None) => {}
        }
    }

    /// Whether the job in the foreground is a different harness than `harness`. False when the
    /// foreground cannot be read, or runs something that is not an agent.
    fn foreground_runs_another(&self, harness: HarnessId) -> bool {
        let Some(source) = &self.foreground else {
            return false;
        };
        matches!(
            source.foreground(),
            Some(Foreground::Job(argv)) if harness_of_job(&argv).is_some_and(|job| job != harness)
        )
    }

    /// The session has an agent now, so the rules start fresh for it.
    fn become_agent(&mut self, harness: HarnessId) {
        self.restart_rules();
        self.seen = false;
        self.write(|slot| {
            slot.harness = Some(harness);
            slot.status = AgentStatus::Idle;
        });
    }

    /// The agent has ended, so the session is a shell again.
    fn become_shell(&mut self) {
        self.restart_rules();
        self.seen = false;
        self.launch_until = None;
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

    /// A foreground that answers from a script: each look takes the next answer, then the last one.
    struct Scripted(Mutex<Vec<Option<Foreground>>>);

    impl Scripted {
        fn new(answers: Vec<Option<Foreground>>) -> Arc<Self> {
            Arc::new(Self(Mutex::new(answers)))
        }
    }

    impl ForegroundSource for Scripted {
        fn foreground(&self) -> Option<Foreground> {
            let mut answers = self.0.lock().expect("lock");
            if answers.len() > 1 {
                answers.remove(0)
            } else {
                answers.first().cloned().flatten()
            }
        }
    }

    /// Waits until the last change the sink saw is `expected`, and returns every change seen.
    fn wait_for_last_change(sink: &StateSink, expected: Change) -> Vec<Change> {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let changes = sink.changes.lock().expect("lock").clone();
            if changes.last() == Some(&expected) || Instant::now() > deadline {
                return changes;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn job(args: &[&str]) -> Option<Foreground> {
        Some(Foreground::Job(
            args.iter().map(|arg| (*arg).to_owned()).collect(),
        ))
    }

    #[test]
    fn a_job_started_from_a_plain_shell_makes_the_session_an_agent() {
        let sink = Arc::new(StateSink::default());
        let link =
            start("s1", None, Arc::clone(&sink) as Arc<dyn TerminalEventSink>).expect("start");
        link.input
            .send(AgentInput::Attach(Scripted::new(vec![
                Some(Foreground::Shell),
                job(&["/bin/sh", "/tmp/fake/claude"]),
            ])))
            .expect("send");

        // The line of output is the command being submitted; the look after it sees the job.
        link.input
            .send(AgentInput::Output("claude\r\n".to_owned()))
            .expect("send");

        assert_eq!(
            wait_for_changes(&sink, 1),
            vec![(Some(HarnessId::Claude), Some(AgentStatus::Idle))]
        );
    }

    #[test]
    fn an_agent_ends_when_its_job_leaves_the_foreground_after_being_seen() {
        let sink = Arc::new(StateSink::default());
        let link = start(
            "s1",
            Some(HarnessId::Claude),
            Arc::clone(&sink) as Arc<dyn TerminalEventSink>,
        )
        .expect("start");
        link.input
            .send(AgentInput::Attach(Scripted::new(vec![
                job(&["claude"]),
                Some(Foreground::Shell),
            ])))
            .expect("send");
        link.input
            .send(AgentInput::Output("working\n".to_owned()))
            .expect("send");
        // Long enough for the next output to be looked at straight away.
        std::thread::sleep(Duration::from_millis(600));
        link.input
            .send(AgentInput::Output("prompt\n".to_owned()))
            .expect("send");

        // Working shows first, because output from an agent is work; the end comes last.
        let changes = wait_for_last_change(&sink, (None, None));
        assert_eq!(changes.last(), Some(&(None, None)), "{changes:?}");
    }

    #[test]
    fn an_agent_from_a_command_is_not_ended_before_its_job_was_seen() {
        let sink = Arc::new(StateSink::default());
        let link = start(
            "s1",
            Some(HarnessId::Claude),
            Arc::clone(&sink) as Arc<dyn TerminalEventSink>,
        )
        .expect("start");
        // The shell has not started the job yet: it is still the foreground.
        link.input
            .send(AgentInput::Attach(Scripted::new(vec![Some(
                Foreground::Shell,
            )])))
            .expect("send");
        link.input
            .send(AgentInput::Output("claude\r\n".to_owned()))
            .expect("send");

        std::thread::sleep(Duration::from_millis(150));
        assert_eq!(
            link.slot.lock().expect("lock").harness,
            Some(HarnessId::Claude)
        );
    }

    #[test]
    fn a_hook_alone_does_not_end_an_agent_before_its_job_is_seen() {
        let sink = Arc::new(StateSink::default());
        let link = start(
            "s1",
            Some(HarnessId::Antigravity),
            Arc::clone(&sink) as Arc<dyn TerminalEventSink>,
        )
        .expect("start");
        link.input
            .send(AgentInput::Hook {
                harness: HarnessId::Antigravity,
                payload: serde_json::json!({ "hook_event_name": "PreInvocation" }),
            })
            .expect("send");
        // The hook's job is not what the foreground shows, so nothing may end the agent.
        link.input
            .send(AgentInput::Attach(Scripted::new(vec![Some(
                Foreground::Shell,
            )])))
            .expect("send");
        link.input
            .send(AgentInput::Output("done\n".to_owned()))
            .expect("send");

        std::thread::sleep(Duration::from_millis(600));
        assert_eq!(
            link.slot.lock().expect("lock").harness,
            Some(HarnessId::Antigravity)
        );
    }
    #[test]
    fn a_hook_from_the_harness_next_door_does_not_make_a_grok_job_a_claude_session() {
        let sink = Arc::new(StateSink::default());
        let link =
            start("s1", None, Arc::clone(&sink) as Arc<dyn TerminalEventSink>).expect("start");
        link.input
            .send(AgentInput::Attach(Scripted::new(vec![job(&["grok"])])))
            .expect("send");

        // Grok relays Claude Code's hook while it runs in this shell.
        link.input
            .send(hook(HarnessId::Claude, "SessionStart"))
            .expect("send");
        std::thread::sleep(Duration::from_millis(150));
        assert_eq!(link.slot.lock().expect("lock").harness, None);

        // Its own hook names it.
        link.input
            .send(hook(HarnessId::Grok, "SessionStart"))
            .expect("send");
        assert_eq!(
            wait_for_changes(&sink, 1),
            vec![(Some(HarnessId::Grok), Some(AgentStatus::Idle))]
        );
    }
}
