//! What an agent session is doing, from the signals its harness and terminal give.
//! Pure: the caller passes the clock, so the rules are tested without sleeping.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use specta::Type;

use super::agent_title::{self, TitleSignal};
use super::harness::HarnessId;

/// What the user sees for an agent session. `Done` means it finished and nobody has looked yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum AgentStatus {
    Working,
    NeedsYou,
    Done,
    Idle,
}

/// A hook event, already translated from the harness's own event names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookEvent {
    /// The harness started a session the user can type into.
    Started,
    /// A turn is running: a prompt was submitted or a tool ran.
    Working,
    /// The harness waits for an answer or an approval.
    NeedsYou,
    /// The turn ended.
    Finished,
    /// The harness session ended. The monitor turns the session back into a shell; the rules
    /// keep no status for it.
    Ended,
}

/// Silence for this long after the last sign of work turns a working session idle.
pub const QUIET_AFTER: Duration = Duration::from_secs(3);

/// Turns the signals of one session into its status. Hooks are the authority once they have
/// spoken; until then, titles and output decide.
#[derive(Debug)]
pub struct AgentActivity {
    status: AgentStatus,
    hooks_seen: bool,
    titles_seen: bool,
    /// The current work is known only from output, so a rest means idle rather than done.
    output_only: bool,
    /// The last sign of progress, for the quiet timer.
    last_progress: Option<Instant>,
}

impl Default for AgentActivity {
    fn default() -> Self {
        Self {
            status: AgentStatus::Idle,
            hooks_seen: false,
            titles_seen: false,
            output_only: false,
            last_progress: None,
        }
    }
}

impl AgentActivity {
    pub fn on_hook(&mut self, event: HookEvent) -> Option<AgentStatus> {
        self.hooks_seen = true;
        self.last_progress = None;
        match event {
            HookEvent::Started => self.change(AgentStatus::Idle, false),
            HookEvent::Working => self.change(AgentStatus::Working, false),
            HookEvent::NeedsYou => self.change(AgentStatus::NeedsYou, false),
            HookEvent::Finished => self.rest(),
            HookEvent::Ended => None,
        }
    }

    /// A title frame. Spinner frames mean work and a rest prefix means the turn is over.
    pub fn on_title(
        &mut self,
        harness: HarnessId,
        title: &str,
        now: Instant,
    ) -> Option<AgentStatus> {
        let signal = agent_title::signal_of(harness, title)?;
        self.titles_seen = true;
        match signal {
            // Hooks say when work starts and stops; a spinner frame adds nothing to that.
            TitleSignal::Working if self.hooks_seen => None,
            TitleSignal::Working => {
                self.last_progress = Some(now);
                self.change(AgentStatus::Working, false)
            }
            TitleSignal::NeedsYou if self.hooks_seen => None,
            TitleSignal::NeedsYou => self.change(AgentStatus::NeedsYou, false),
            TitleSignal::Rest => self.rest(),
        }
    }

    /// Terminal output. It counts as work only when nothing better is known about the session.
    pub fn on_output(&mut self, now: Instant) -> Option<AgentStatus> {
        if self.hooks_seen || self.titles_seen {
            return None;
        }
        self.last_progress = Some(now);
        self.change(AgentStatus::Working, true)
    }

    /// Called on a timer. Work with no sign of progress for a while reads as idle, not done:
    /// nothing said the turn finished. Hooks skip this, since a long tool run is still work.
    pub fn on_tick(&mut self, now: Instant) -> Option<AgentStatus> {
        if self.hooks_seen || self.status != AgentStatus::Working {
            return None;
        }
        let progressed = self.last_progress?;
        if now.duration_since(progressed) < QUIET_AFTER {
            return None;
        }
        self.change(AgentStatus::Idle, false)
    }

    /// The user looked at the session, so a finished turn is no longer news.
    pub fn mark_seen(&mut self) -> Option<AgentStatus> {
        if self.status == AgentStatus::Done {
            self.change(AgentStatus::Idle, false)
        } else {
            None
        }
    }

    /// The turn ended or the agent is at its prompt again.
    fn rest(&mut self) -> Option<AgentStatus> {
        match self.status {
            AgentStatus::Working | AgentStatus::NeedsYou => {
                let next = if self.output_only {
                    AgentStatus::Idle
                } else {
                    AgentStatus::Done
                };
                self.change(next, false)
            }
            AgentStatus::Done | AgentStatus::Idle => None,
        }
    }

    fn change(&mut self, next: AgentStatus, output_only: bool) -> Option<AgentStatus> {
        self.output_only = output_only;
        if self.status == next {
            return None;
        }
        self.status = next;
        Some(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn start() -> Instant {
        Instant::now()
    }

    #[test]
    fn hooks_drive_working_and_finished_turns() {
        let mut activity = AgentActivity::default();
        assert_eq!(
            activity.on_hook(HookEvent::Working),
            Some(AgentStatus::Working)
        );
        assert_eq!(
            activity.on_hook(HookEvent::Finished),
            Some(AgentStatus::Done)
        );
    }

    #[test]
    fn a_finished_hook_on_an_idle_session_changes_nothing() {
        let mut activity = AgentActivity::default();
        assert_eq!(activity.on_hook(HookEvent::Finished), None);
        assert_eq!(activity.status, AgentStatus::Idle);
    }

    #[test]
    fn a_permission_request_needs_you_until_the_tool_resumes() {
        let mut activity = AgentActivity::default();
        activity.on_hook(HookEvent::Working);
        assert_eq!(
            activity.on_hook(HookEvent::NeedsYou),
            Some(AgentStatus::NeedsYou)
        );
        assert_eq!(
            activity.on_hook(HookEvent::Working),
            Some(AgentStatus::Working)
        );
    }

    #[test]
    fn a_spinner_title_is_working_and_the_rest_prefix_is_done() {
        let now = start();
        let mut activity = AgentActivity::default();
        assert_eq!(
            activity.on_title(HarnessId::Claude, "⠋ Claude Code", now),
            Some(AgentStatus::Working)
        );
        assert_eq!(
            activity.on_title(HarnessId::Claude, "✳ Claude Code", now),
            Some(AgentStatus::Done)
        );
    }

    #[test]
    fn the_ascii_rest_prefix_counts_as_rest_too() {
        let now = start();
        let mut activity = AgentActivity::default();
        activity.on_title(HarnessId::Claude, "⠋ Claude Code", now);
        assert_eq!(
            activity.on_title(HarnessId::Claude, "* Claude Code", now),
            Some(AgentStatus::Done)
        );
    }

    #[test]
    fn a_title_that_says_nothing_about_the_agent_is_ignored() {
        let now = start();
        let mut activity = AgentActivity::default();
        assert_eq!(
            activity.on_title(HarnessId::Claude, "~/projects/repo", now),
            None
        );
        assert_eq!(activity.on_title(HarnessId::Claude, "zsh", now), None);
        assert_eq!(activity.status, AgentStatus::Idle);
    }

    #[test]
    fn an_action_required_title_needs_you() {
        let now = start();
        let mut activity = AgentActivity::default();
        assert_eq!(
            activity.on_title(HarnessId::Claude, "Claude Code - action required", now),
            Some(AgentStatus::NeedsYou)
        );
    }

    #[test]
    fn hooks_ignore_spinner_titles_but_still_see_the_rest_prefix() {
        let now = start();
        let mut activity = AgentActivity::default();
        activity.on_hook(HookEvent::Working);
        assert_eq!(
            activity.on_title(HarnessId::Claude, "⠋ Claude Code", now),
            None
        );
        // An interrupted turn sends no Stop hook; the rest title is what ends it.
        assert_eq!(
            activity.on_title(HarnessId::Claude, "✳ Claude Code", now),
            Some(AgentStatus::Done)
        );
    }

    #[test]
    fn a_working_title_that_goes_quiet_turns_idle_not_done() {
        let now = start();
        let mut activity = AgentActivity::default();
        activity.on_title(HarnessId::Claude, "⠋ Codex", now);
        assert_eq!(activity.on_tick(now + Duration::from_secs(1)), None);
        assert_eq!(activity.on_tick(now + QUIET_AFTER), Some(AgentStatus::Idle));
    }

    #[test]
    fn hooks_never_time_out_a_long_tool_run() {
        let now = start();
        let mut activity = AgentActivity::default();
        activity.on_hook(HookEvent::Working);
        assert_eq!(activity.on_tick(now + Duration::from_secs(600)), None);
        assert_eq!(activity.status, AgentStatus::Working);
    }

    #[test]
    fn output_alone_shows_work_and_quiet_goes_idle_without_claiming_done() {
        let now = start();
        let mut activity = AgentActivity::default();
        assert_eq!(activity.on_output(now), Some(AgentStatus::Working));
        assert_eq!(activity.on_tick(now + QUIET_AFTER), Some(AgentStatus::Idle));
    }

    #[test]
    fn a_rest_after_output_only_work_is_idle_not_done() {
        // A launch banner is output, not a finished turn.
        let now = start();
        let mut activity = AgentActivity::default();
        activity.on_output(now);
        assert_eq!(
            activity.on_title(HarnessId::Claude, "✳ Claude Code", now),
            Some(AgentStatus::Idle)
        );
    }

    #[test]
    fn output_is_ignored_once_a_title_or_hook_has_spoken() {
        let now = start();
        let mut activity = AgentActivity::default();
        activity.on_title(HarnessId::Claude, "✳ Claude Code", now);
        assert_eq!(activity.on_output(now), None);
        assert_eq!(activity.status, AgentStatus::Idle);
    }

    #[test]
    fn mark_seen_clears_done_and_nothing_else() {
        let mut activity = AgentActivity::default();
        activity.on_hook(HookEvent::Working);
        assert_eq!(activity.mark_seen(), None);
        activity.on_hook(HookEvent::Finished);
        assert_eq!(activity.mark_seen(), Some(AgentStatus::Idle));
        assert_eq!(activity.status, AgentStatus::Idle);
    }
}
