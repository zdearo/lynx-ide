//! Waking an agent coordinator when a worker of its run reports or needs an answer. The note is typed into
//! the coordinator's terminal, but only when the agent is idle and the person is not halfway through a message:
//! a coordinator busy in a turn gets the note when the turn ends (its `Stop` hook changes the status), and a
//! message the person is writing waits until they submit it.

use std::collections::{HashMap, HashSet};
use std::iter::Peekable;
use std::str::Chars;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use crate::agents::activity::AgentStatus;
use crate::terminal::TerminalError;
use crate::terminal::terminal_manager::TerminalManager;

use super::host::WAKE_NOTE;

/// How long a note waits for its coordinator before it is dropped. The check tool still reads the workers then.
pub const DEADLINE: Duration = Duration::from_secs(15 * 60);

/// The pause between the text and its Enter, the same one the team's own messages use.
const SUBMIT_DELAY: Duration = Duration::from_millis(150);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Nothing is waiting for this session.
    Nothing,
    /// The agent is busy, or the person has a draft in its input.
    Wait,
    /// The agent is idle and free: type the note now.
    Deliver,
    /// The note can no longer be delivered: the session ended, or the wait ran out.
    Forget,
}

/// What is waiting for each coordinator, and which sessions the person has typed into without submitting.
#[derive(Default)]
pub struct WakeBook {
    since: HashMap<String, Instant>,
    draft: HashSet<String>,
}

impl WakeBook {
    /// A note for the session. Several reports while it waits make one note.
    pub fn note(&mut self, session: &str, now: Instant) {
        self.since.entry(session.to_owned()).or_insert(now);
    }

    pub fn is_pending(&self, session: &str) -> bool {
        self.since.contains_key(session)
    }

    /// Drops the note: it was delivered, or the coordinator has read its workers itself.
    pub fn cancel(&mut self, session: &str) {
        self.since.remove(session);
    }

    /// Records what the person typed into a session, so a draft is known before the next note.
    pub fn typed(&mut self, session: &str, data: &str) {
        if draft_after(self.draft.contains(session), data) {
            self.draft.insert(session.to_owned());
        } else {
            self.draft.remove(session);
        }
    }

    /// The session ended: its note and its draft go with it.
    pub fn closed(&mut self, session: &str) {
        self.since.remove(session);
        self.draft.remove(session);
    }

    /// What to do with the note of `session` now.
    pub fn decide(
        &self,
        session: &str,
        running: bool,
        status: Option<AgentStatus>,
        now: Instant,
    ) -> Step {
        let Some(since) = self.since.get(session) else {
            return Step::Nothing;
        };
        if !running || now.saturating_duration_since(*since) > DEADLINE {
            return Step::Forget;
        }
        if self.draft.contains(session) {
            return Step::Wait;
        }
        match status {
            Some(AgentStatus::Idle | AgentStatus::Done) => Step::Deliver,
            Some(AgentStatus::Working | AgentStatus::NeedsYou) | None => Step::Wait,
        }
    }
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

/// The wake notes of the agent coordinators, shared by the event sink, the tools and the terminal writes.
pub struct AgentWakes {
    book: Mutex<WakeBook>,
    terminals: TerminalManager,
}

impl AgentWakes {
    pub fn new(terminals: TerminalManager) -> Self {
        Self {
            book: Mutex::new(WakeBook::default()),
            terminals,
        }
    }

    fn book(&self) -> MutexGuard<'_, WakeBook> {
        self.book.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A worker of the coordinator's run reported or needs an answer.
    pub fn note(self: &Arc<Self>, coordinator: &str) {
        self.book().note(coordinator, Instant::now());
        self.attempt_later(coordinator);
    }

    /// The person typed into a session. Only a note that waits is looked at again, so typing costs nothing else.
    pub fn typed(self: &Arc<Self>, session: &str, data: &str) {
        let mut book = self.book();
        book.typed(session, data);
        let pending = book.is_pending(session);
        drop(book);
        if pending {
            self.attempt_later(session);
        }
    }

    /// The status of a session changed, so a note waiting for it may be deliverable now.
    pub fn status_changed(self: &Arc<Self>, session: &str) {
        self.attempt_later(session);
    }

    /// The coordinator read its workers itself, so the note would repeat what it already knows.
    pub fn cancel(&self, session: &str) {
        self.book().cancel(session);
    }

    pub fn closed(&self, session: &str) {
        self.book().closed(session);
    }

    /// Tries the note on its own thread: the status event comes from a monitor that must not wait on a terminal.
    fn attempt_later(self: &Arc<Self>, session: &str) {
        if !self.book().is_pending(session) {
            return;
        }
        let wakes = Arc::clone(self);
        let session = session.to_owned();
        std::thread::spawn(move || wakes.attempt(&session));
    }

    fn attempt(&self, session: &str) {
        let info = self.terminals.info(session).ok();
        let running = info.as_ref().is_some_and(|info| info.running);
        let status = info.and_then(|info| info.agent_status);
        // Deciding and dropping the note happen under one lock, so two attempts cannot both type it.
        let step = {
            let mut book = self.book();
            let step = book.decide(session, running, status, Instant::now());
            if matches!(step, Step::Deliver | Step::Forget) {
                book.cancel(session);
            }
            step
        };
        if step == Step::Deliver {
            self.type_note(session);
        }
    }

    /// Types the note and presses Enter, as the person would.
    fn type_note(&self, session: &str) {
        if let Err(error) = type_and_submit(&self.terminals, session, WAKE_NOTE) {
            log::error!("could not wake coordinator {session}: {error}");
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

#[cfg(test)]
mod tests {
    use super::*;

    const S: &str = "coordinator";

    fn start() -> Instant {
        Instant::now()
    }

    #[test]
    fn an_idle_coordinator_is_woken_at_once() {
        let t = start();
        let mut book = WakeBook::default();
        book.note(S, t);
        assert_eq!(
            book.decide(S, true, Some(AgentStatus::Idle), t),
            Step::Deliver
        );
    }

    #[test]
    fn a_coordinator_that_is_done_with_its_turn_is_woken_too() {
        let t = start();
        let mut book = WakeBook::default();
        book.note(S, t);
        assert_eq!(
            book.decide(S, true, Some(AgentStatus::Done), t),
            Step::Deliver
        );
    }

    #[test]
    fn a_busy_or_waiting_coordinator_is_not_interrupted() {
        let t = start();
        let mut book = WakeBook::default();
        book.note(S, t);
        assert_eq!(
            book.decide(S, true, Some(AgentStatus::Working), t),
            Step::Wait
        );
        assert_eq!(
            book.decide(S, true, Some(AgentStatus::NeedsYou), t),
            Step::Wait
        );
        assert_eq!(
            book.decide(S, true, None, t),
            Step::Wait,
            "no status is not idle"
        );
    }

    #[test]
    fn a_note_waits_for_the_turn_to_end_and_is_delivered_then() {
        let t = start();
        let mut book = WakeBook::default();
        book.note(S, t);
        assert_eq!(
            book.decide(S, true, Some(AgentStatus::Working), t),
            Step::Wait
        );
        // The Stop hook moves the status to done: the same note is delivered.
        assert_eq!(
            book.decide(S, true, Some(AgentStatus::Done), t),
            Step::Deliver
        );
    }

    #[test]
    fn many_reports_make_one_note() {
        let t = start();
        let mut book = WakeBook::default();
        book.note(S, t);
        book.note(S, t + Duration::from_secs(5));
        book.note(S, t + Duration::from_secs(9));
        assert_eq!(
            book.decide(S, true, Some(AgentStatus::Idle), t),
            Step::Deliver
        );
        book.cancel(S);
        assert_eq!(
            book.decide(S, true, Some(AgentStatus::Idle), t),
            Step::Nothing
        );
    }

    #[test]
    fn a_note_keeps_its_first_time_so_the_deadline_is_not_pushed_back() {
        let t = start();
        let mut book = WakeBook::default();
        book.note(S, t);
        book.note(S, t + DEADLINE);
        let late = t + DEADLINE + Duration::from_secs(1);
        assert_eq!(
            book.decide(S, true, Some(AgentStatus::Working), late),
            Step::Forget
        );
    }

    #[test]
    fn a_draft_in_the_input_holds_the_note_until_it_is_submitted() {
        let t = start();
        let mut book = WakeBook::default();
        book.note(S, t);
        book.typed(S, "quero saber");
        assert_eq!(book.decide(S, true, Some(AgentStatus::Idle), t), Step::Wait);
        book.typed(S, "\r");
        assert_eq!(
            book.decide(S, true, Some(AgentStatus::Idle), t),
            Step::Deliver
        );
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
    fn a_draft_typed_in_one_session_does_not_hold_another() {
        let t = start();
        let mut book = WakeBook::default();
        book.note("b", t);
        book.typed("a", "rascunho");
        assert_eq!(
            book.decide("b", true, Some(AgentStatus::Idle), t),
            Step::Deliver
        );
    }

    #[test]
    fn a_closed_session_forgets_its_note_and_its_draft() {
        let t = start();
        let mut book = WakeBook::default();
        book.note(S, t);
        book.typed(S, "meio");
        book.closed(S);
        assert_eq!(
            book.decide(S, false, Some(AgentStatus::Idle), t),
            Step::Nothing
        );
        book.note(S, t);
        assert_eq!(
            book.decide(S, true, Some(AgentStatus::Idle), t),
            Step::Deliver,
            "no draft is left"
        );
    }

    #[test]
    fn a_session_that_is_no_longer_running_is_forgotten() {
        let t = start();
        let mut book = WakeBook::default();
        book.note(S, t);
        assert_eq!(
            book.decide(S, false, Some(AgentStatus::Idle), t),
            Step::Forget
        );
    }

    #[test]
    fn a_note_past_the_deadline_is_forgotten_even_when_the_agent_is_idle() {
        let t = start();
        let mut book = WakeBook::default();
        book.note(S, t);
        let late = t + DEADLINE + Duration::from_secs(1);
        assert_eq!(
            book.decide(S, true, Some(AgentStatus::Idle), late),
            Step::Forget
        );
    }

    #[test]
    fn without_a_note_nothing_happens() {
        let t = start();
        let book = WakeBook::default();
        assert_eq!(
            book.decide(S, true, Some(AgentStatus::Idle), t),
            Step::Nothing
        );
    }
}
