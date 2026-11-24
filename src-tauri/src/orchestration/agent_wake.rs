//! Handing a coordinator agent the reports its workers sent. A report waits in the team until a hand-over reads it.
//! The reports are pasted into the coordinator's input only when its agent is idle (its `Stop` hook moved it to
//! `Done`) and the person has no draft there; a busy coordinator gets them when its turn ends. One paste carries
//! every report that waits, so the coordinator reads them as one message.

use std::collections::HashSet;
use std::iter::Peekable;
use std::str::Chars;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use crate::agents::activity::AgentStatus;
use crate::terminal::TerminalError;
use crate::terminal::pasted_text::as_paste;
use crate::terminal::terminal_manager::TerminalManager;

use super::reports::{ReportSource, Reports};

/// The pause between the text and its Enter, the same one the team's own messages use.
const SUBMIT_DELAY: Duration = Duration::from_millis(150);

/// What a session's agent shows now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Live {
    pub running: bool,
    pub status: Option<AgentStatus>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// No reports wait for this session.
    Nothing,
    /// The agent is busy, or the person has a draft in its input.
    Wait,
    /// The agent is idle and free: paste the reports now.
    Deliver,
    /// The session ended. Its reports stay in the team, for a check to read.
    Forget,
}

/// What one attempt did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Delivery {
    Nothing,
    Waiting,
    Forgotten,
    /// The reports went in, with this many messages.
    Delivered(usize),
    /// The paste failed: the reports are unread again, and the next status change tries again.
    Undelivered(String),
}

/// Which coordinators have reports waiting, and which sessions the person has typed into without submitting.
#[derive(Default)]
pub struct CoordinatorGate {
    waiting: HashSet<String>,
    draft: HashSet<String>,
}

impl CoordinatorGate {
    pub fn wait(&mut self, session: &str) {
        self.waiting.insert(session.to_owned());
    }

    pub fn is_waiting(&self, session: &str) -> bool {
        self.waiting.contains(session)
    }

    /// Clears the wait: the reports were handed over, or none were there.
    pub fn done_waiting(&mut self, session: &str) {
        self.waiting.remove(session);
    }

    /// Records what the person typed into a session, so a draft is known before the next hand-over.
    pub fn typed(&mut self, session: &str, data: &str) {
        if draft_after(self.draft.contains(session), data) {
            self.draft.insert(session.to_owned());
        } else {
            self.draft.remove(session);
        }
    }

    /// The session ended: its wait and its draft go with it.
    pub fn closed(&mut self, session: &str) {
        self.waiting.remove(session);
        self.draft.remove(session);
    }

    /// Whether the person has typed a draft into `session` without submitting it.
    pub fn has_draft(&self, session: &str) -> bool {
        self.draft.contains(session)
    }

    /// What to do with the reports of `session` now.
    pub fn decide(&self, session: &str, live: Live) -> Step {
        if !self.waiting.contains(session) {
            return Step::Nothing;
        }
        if !live.running {
            return Step::Forget;
        }
        if self.draft.contains(session) {
            return Step::Wait;
        }
        match live.status {
            Some(AgentStatus::Idle | AgentStatus::Done) => Step::Deliver,
            Some(AgentStatus::Working | AgentStatus::NeedsYou) | None => Step::Wait,
        }
    }
}

/// Hands coordinators their reports, one hand-over at a time.
pub struct ReportDelivery {
    gate: Mutex<CoordinatorGate>,
    /// Held for a whole attempt, so two attempts cannot take and paste reports for the same coordinator at once.
    turn: Mutex<()>,
    reports: Arc<dyn ReportSource>,
}

impl ReportDelivery {
    pub fn new(reports: Arc<dyn ReportSource>) -> Self {
        Self {
            gate: Mutex::new(CoordinatorGate::default()),
            turn: Mutex::new(()),
            reports,
        }
    }

    fn gate(&self) -> MutexGuard<'_, CoordinatorGate> {
        self.gate.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Reports were written for the coordinator in `session`.
    pub fn note(&self, session: &str) {
        self.gate().wait(session);
    }

    pub fn is_waiting(&self, session: &str) -> bool {
        self.gate().is_waiting(session)
    }

    pub fn typed(&self, session: &str, data: &str) {
        self.gate().typed(session, data);
    }

    pub fn closed(&self, session: &str) {
        self.gate().closed(session);
    }

    pub fn has_draft(&self, session: &str) -> bool {
        self.gate().has_draft(session)
    }

    /// Hands the reports of `session` over, if it can take them now. `live` is read under the turn lock, so the
    /// status it gives is the one left by the last hand-over. `type_text` pastes a text and presses Enter.
    pub fn attempt(
        &self,
        session: &str,
        live: impl FnOnce() -> Live,
        type_text: &mut dyn FnMut(&str) -> Result<(), String>,
    ) -> Delivery {
        let _turn = self.turn.lock().unwrap_or_else(PoisonError::into_inner);
        let step = {
            let mut gate = self.gate();
            let step = gate.decide(session, live());
            if step == Step::Forget {
                gate.done_waiting(session);
            }
            step
        };
        match step {
            Step::Nothing => Delivery::Nothing,
            Step::Wait => Delivery::Waiting,
            Step::Forget => Delivery::Forgotten,
            Step::Deliver => {
                // Cleared before the reports are taken: a report that arrives during the paste asks for another attempt.
                self.gate().done_waiting(session);
                let Some(reports) = self.reports.take(session) else {
                    return Delivery::Nothing;
                };
                match type_text(&reports.text) {
                    Ok(()) => {
                        self.reports.sent(&reports);
                        Delivery::Delivered(reports.messages.len())
                    }
                    Err(error) => {
                        self.undelivered(session, &reports);
                        Delivery::Undelivered(error)
                    }
                }
            }
        }
    }

    fn undelivered(&self, session: &str, reports: &Reports) {
        self.reports.undelivered(reports);
        self.gate().wait(session);
    }
}

/// The reports of the coordinators, handed to their agents through their terminals.
pub struct AgentWakes {
    terminals: TerminalManager,
    deliveries: ReportDelivery,
}

impl AgentWakes {
    pub fn new(terminals: TerminalManager, reports: Arc<dyn ReportSource>) -> Self {
        Self {
            terminals,
            deliveries: ReportDelivery::new(reports),
        }
    }

    /// A report was written for the coordinator in `session`.
    pub fn note(self: &Arc<Self>, session: &str) {
        self.deliveries.note(session);
        self.attempt_later(session);
    }

    /// The person typed into a session. Only a session with reports waiting is looked at again.
    pub fn typed(self: &Arc<Self>, session: &str, data: &str) {
        self.deliveries.typed(session, data);
        if self.deliveries.is_waiting(session) {
            self.attempt_later(session);
        }
    }

    /// The status of a session changed, so its reports may go in now.
    pub fn status_changed(self: &Arc<Self>, session: &str) {
        self.attempt_later(session);
    }

    pub fn closed(&self, session: &str) {
        self.deliveries.closed(session);
    }

    /// Whether the person has a draft in `session`'s input, which holds back anything typed into it.
    pub fn has_draft(&self, session: &str) -> bool {
        self.deliveries.has_draft(session)
    }

    /// Tries the hand-over on its own thread: the status event comes from a monitor that must not wait on a terminal.
    fn attempt_later(self: &Arc<Self>, session: &str) {
        if !self.deliveries.is_waiting(session) {
            return;
        }
        let wakes = Arc::clone(self);
        let session = session.to_owned();
        std::thread::spawn(move || wakes.attempt(&session));
    }

    fn attempt(&self, session: &str) {
        let outcome = self
            .deliveries
            .attempt(session, || self.live(session), &mut |text| {
                type_and_submit(&self.terminals, session, &as_paste(text))
                    .map_err(|error| error.to_string())
            });
        if let Delivery::Undelivered(error) = outcome {
            log::error!("could not hand the reports to coordinator {session}: {error}");
        }
    }

    fn live(&self, session: &str) -> Live {
        let info = self.terminals.info(session).ok();
        Live {
            running: info.as_ref().is_some_and(|info| info.running),
            status: info.and_then(|info| info.agent_status),
        }
    }
}

/// Types `text` and presses Enter. The Enter is a write of its own: a program that reads a paste takes an Enter
/// that arrives with the text as more text, and the message would stay in its input unsent.
pub(super) fn type_and_submit(
    terminals: &TerminalManager,
    session: &str,
    text: &str,
) -> Result<(), TerminalError> {
    terminals.write(session, text)?;
    std::thread::sleep(SUBMIT_DELAY);
    terminals.write(session, "\r")
}

/// Whether a question may be typed into a session now: its agent is idle or done, and the person has no draft.
pub fn free_to_type(status: Option<AgentStatus>, draft: bool) -> bool {
    !draft && matches!(status, Some(AgentStatus::Idle | AgentStatus::Done))
}

/// Whether the input has a draft after the person typed `data`: printable text that was not submitted.
/// Enter submits, and Ctrl+C and Ctrl+U clear the line. Escape sequences (arrows, paste markers) are not text.
pub fn draft_after(draft: bool, data: &str) -> bool {
    let mut draft = draft;
    let mut chars = data.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\u{1b}' => skip_escape(&mut chars),
            '\r' | '\n' | '\u{3}' | '\u{15}' => draft = false,
            c if !c.is_control() => draft = true,
            _ => {}
        }
    }
    draft
}

/// Skips the rest of an escape sequence: `ESC [ ... final` or a two-character `ESC x`.
fn skip_escape(chars: &mut Peekable<Chars<'_>>) {
    if chars.peek() == Some(&'[') {
        chars.next();
        for c in chars.by_ref() {
            if ('@'..='~').contains(&c) {
                break;
            }
        }
    } else {
        chars.next();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::team::team_model::{MessageKind, TeamMessage};

    const S: &str = "coordinator";

    fn live(status: Option<AgentStatus>) -> Live {
        Live {
            running: true,
            status,
        }
    }

    fn message(id: &str) -> TeamMessage {
        TeamMessage {
            id: id.to_owned(),
            member_id: "worker".to_owned(),
            kind: MessageKind::Done,
            body: "guia".to_owned(),
            created_at: 0,
        }
    }

    fn reports(ids: &[&str], text: &str) -> Reports {
        Reports {
            messages: ids.iter().map(|id| message(id)).collect(),
            text: text.to_owned(),
        }
    }

    /// The reports waiting in the team: a hand-over takes the first, and a failed one puts it back.
    #[derive(Default)]
    struct FakeReports {
        waiting: Mutex<Vec<Reports>>,
        sent: Mutex<usize>,
        returned: Mutex<usize>,
    }

    impl FakeReports {
        fn with(waiting: Vec<Reports>) -> Self {
            Self {
                waiting: Mutex::new(waiting),
                ..Self::default()
            }
        }
    }

    impl ReportSource for FakeReports {
        fn take(&self, _session: &str) -> Option<Reports> {
            let mut waiting = self.waiting.lock().unwrap();
            (!waiting.is_empty()).then(|| waiting.remove(0))
        }

        fn sent(&self, _reports: &Reports) {
            *self.sent.lock().unwrap() += 1;
        }

        fn undelivered(&self, reports: &Reports) {
            *self.returned.lock().unwrap() += 1;
            self.waiting.lock().unwrap().insert(0, reports.clone());
        }
    }

    /// A paste that goes through, recording each text it was given.
    fn typing(typed: &mut Vec<String>) -> impl FnMut(&str) -> Result<(), String> + '_ {
        move |text| {
            typed.push(text.to_owned());
            Ok(())
        }
    }

    #[test]
    fn an_idle_or_done_coordinator_takes_its_reports_and_a_busy_one_does_not() {
        let mut gate = CoordinatorGate::default();
        gate.wait(S);
        assert_eq!(gate.decide(S, live(Some(AgentStatus::Idle))), Step::Deliver);
        assert_eq!(gate.decide(S, live(Some(AgentStatus::Done))), Step::Deliver);
        assert_eq!(gate.decide(S, live(Some(AgentStatus::Working))), Step::Wait);
        assert_eq!(
            gate.decide(S, live(Some(AgentStatus::NeedsYou))),
            Step::Wait
        );
        assert_eq!(
            gate.decide(S, live(None)),
            Step::Wait,
            "no status is not idle"
        );
    }

    #[test]
    fn a_session_with_no_reports_waiting_has_nothing_to_do_and_an_ended_one_is_forgotten() {
        let mut gate = CoordinatorGate::default();
        assert_eq!(gate.decide(S, live(Some(AgentStatus::Idle))), Step::Nothing);
        gate.wait(S);
        let ended = Live {
            running: false,
            status: Some(AgentStatus::Idle),
        };
        assert_eq!(gate.decide(S, ended), Step::Forget);
    }

    #[test]
    fn a_draft_in_the_input_holds_the_reports_until_it_is_submitted() {
        let mut gate = CoordinatorGate::default();
        gate.wait(S);
        gate.typed(S, "quero saber");
        assert_eq!(gate.decide(S, live(Some(AgentStatus::Idle))), Step::Wait);
        gate.typed(S, "\r");
        assert_eq!(gate.decide(S, live(Some(AgentStatus::Idle))), Step::Deliver);
    }

    #[test]
    fn a_draft_typed_in_one_session_does_not_hold_another() {
        let mut gate = CoordinatorGate::default();
        gate.wait("b");
        gate.typed("a", "rascunho");
        assert_eq!(
            gate.decide("b", live(Some(AgentStatus::Idle))),
            Step::Deliver
        );
    }

    #[test]
    fn a_closed_session_forgets_its_wait_and_its_draft() {
        let mut gate = CoordinatorGate::default();
        gate.wait(S);
        gate.typed(S, "meio");
        gate.closed(S);
        assert!(!gate.is_waiting(S));
        assert!(!gate.has_draft(S));
    }

    #[test]
    fn text_and_submit_in_one_write_leaves_no_draft_but_text_after_it_does() {
        assert!(!draft_after(false, "oi\r"), "text then enter: submitted");
        assert!(
            draft_after(false, "oi\rmais"),
            "a new draft after the submit"
        );
        assert!(draft_after(true, "x"));
        assert!(!draft_after(true, "\r"));
    }

    #[test]
    fn ctrl_c_and_ctrl_u_clear_a_draft() {
        assert!(!draft_after(true, "\u{15}"));
        assert!(!draft_after(true, "\u{3}"));
        assert!(draft_after(true, "\u{15}ola"));
    }

    #[test]
    fn arrows_and_paste_markers_are_not_text_but_pasted_text_is() {
        assert!(!draft_after(false, "\u{1b}[A"), "up arrow");
        assert!(!draft_after(false, "\u{1b}[D\u{1b}[C"), "left and right");
        assert!(
            draft_after(false, "\u{1b}[200~colado\u{1b}[201~"),
            "bracketed paste"
        );
        assert!(!draft_after(false, "\u{7f}"), "backspace alone is not text");
    }

    #[test]
    fn an_idle_coordinator_gets_its_reports_once() {
        let source = Arc::new(FakeReports::with(vec![reports(&["m1"], "relatório um")]));
        let delivery = ReportDelivery::new(source.clone());
        let mut typed = Vec::new();
        delivery.note(S);

        let first = delivery.attempt(S, || live(Some(AgentStatus::Idle)), &mut typing(&mut typed));
        let second = delivery.attempt(S, || live(Some(AgentStatus::Idle)), &mut typing(&mut typed));

        assert_eq!(first, Delivery::Delivered(1));
        assert_eq!(second, Delivery::Nothing, "nothing is owed twice");
        assert_eq!(typed, vec!["relatório um".to_owned()]);
        assert_eq!(*source.sent.lock().unwrap(), 1);
    }

    #[test]
    fn a_busy_coordinator_gets_its_reports_when_its_turn_ends() {
        let source = Arc::new(FakeReports::with(vec![reports(&["m1", "m2"], "dois")]));
        let delivery = ReportDelivery::new(source);
        let mut typed = Vec::new();
        delivery.note(S);

        let busy = delivery.attempt(
            S,
            || live(Some(AgentStatus::Working)),
            &mut typing(&mut typed),
        );
        assert_eq!(busy, Delivery::Waiting);
        assert!(typed.is_empty(), "nothing goes into a busy agent");

        let done = delivery.attempt(S, || live(Some(AgentStatus::Done)), &mut typing(&mut typed));
        assert_eq!(done, Delivery::Delivered(2));
        assert_eq!(typed.len(), 1);
    }

    #[test]
    fn several_reports_go_in_one_paste() {
        let source = Arc::new(FakeReports::with(vec![reports(
            &["m1", "m2", "m3"],
            "tres",
        )]));
        let delivery = ReportDelivery::new(source);
        let mut typed = Vec::new();
        delivery.note(S);

        delivery.attempt(S, || live(Some(AgentStatus::Idle)), &mut typing(&mut typed));

        assert_eq!(typed, vec!["tres".to_owned()]);
    }

    #[test]
    fn a_paste_that_fails_puts_the_reports_back_and_the_next_status_tries_again() {
        let source = Arc::new(FakeReports::with(vec![reports(&["m1"], "relatório")]));
        let delivery = ReportDelivery::new(source.clone());
        delivery.note(S);

        let failed = delivery.attempt(S, || live(Some(AgentStatus::Idle)), &mut |_| {
            Err("the terminal is gone".to_owned())
        });
        assert_eq!(
            failed,
            Delivery::Undelivered("the terminal is gone".to_owned())
        );
        assert_eq!(*source.returned.lock().unwrap(), 1);
        assert_eq!(*source.sent.lock().unwrap(), 0);
        assert!(delivery.is_waiting(S), "the next status change tries again");

        let mut typed = Vec::new();
        let retried =
            delivery.attempt(S, || live(Some(AgentStatus::Idle)), &mut typing(&mut typed));
        assert_eq!(retried, Delivery::Delivered(1));
        assert_eq!(
            typed,
            vec!["relatório".to_owned()],
            "the same report, once more"
        );
    }

    #[test]
    fn reports_a_check_already_read_are_not_pasted() {
        let source = Arc::new(FakeReports::default());
        let delivery = ReportDelivery::new(source);
        let mut typed = Vec::new();
        delivery.note(S);

        let outcome =
            delivery.attempt(S, || live(Some(AgentStatus::Idle)), &mut typing(&mut typed));

        assert_eq!(outcome, Delivery::Nothing);
        assert!(typed.is_empty());
    }

    #[test]
    fn a_draft_in_the_input_holds_the_reports_back() {
        let source = Arc::new(FakeReports::with(vec![reports(&["m1"], "relatório")]));
        let delivery = ReportDelivery::new(source);
        let mut typed = Vec::new();
        delivery.note(S);
        delivery.typed(S, "rascunho");

        let outcome =
            delivery.attempt(S, || live(Some(AgentStatus::Idle)), &mut typing(&mut typed));

        assert_eq!(outcome, Delivery::Waiting);
        assert!(typed.is_empty());
    }

    #[test]
    fn a_coordinator_whose_session_ended_gets_nothing_and_keeps_its_reports_for_a_check() {
        let source = Arc::new(FakeReports::with(vec![reports(&["m1"], "relatório")]));
        let delivery = ReportDelivery::new(source.clone());
        let mut typed = Vec::new();
        delivery.note(S);

        let ended = Live {
            running: false,
            status: None,
        };
        let outcome = delivery.attempt(S, || ended, &mut typing(&mut typed));

        assert_eq!(outcome, Delivery::Forgotten);
        assert!(typed.is_empty());
        assert_eq!(
            source.waiting.lock().unwrap().len(),
            1,
            "still unread in the team"
        );
        assert!(!delivery.is_waiting(S));
    }
}
