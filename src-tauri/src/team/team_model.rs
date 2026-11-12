//! The team's data: the members of each worktree, the lines of conversation between them, and the work
//! each worker member does. A worktree's team is the agents its terminal tabs show; nothing links two
//! worktrees' teams. A worker is the member that was given the task, so the tree shows the work and who does it.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::agents::harness::HarnessId;

/// A CLI agent in a worktree, or a Lynx bot. A bot only orchestrates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum TeamMemberKind {
    Agent,
    Bot,
}

impl TeamMemberKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Bot => "bot",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "agent" => Some(Self::Agent),
            "bot" => Some(Self::Bot),
            _ => None,
        }
    }
}

/// Where a member's task stands. A member with an orchestrator above it has one, and it starts pending.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum TaskState {
    Pending,
    Running,
    Done,
    Failed,
    Cancelled,
}

impl TaskState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "pending" => Some(Self::Pending),
            "running" => Some(Self::Running),
            "done" => Some(Self::Done),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

/// Whether a started worker is still in charge of its task. A stopped worker keeps its session for the record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum DispatchState {
    Running,
    Stopped,
}

impl DispatchState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Stopped => "stopped",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "running" => Some(Self::Running),
            "stopped" => Some(Self::Stopped),
            _ => None,
        }
    }
}

/// How a worker says its task ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Done,
    Failed,
}

impl Outcome {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "done" => Some(Self::Done),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }

    pub fn task_state(self) -> TaskState {
        match self {
            Self::Done => TaskState::Done,
            Self::Failed => TaskState::Failed,
        }
    }

    pub fn message_kind(self) -> MessageKind {
        match self {
            Self::Done => MessageKind::Done,
            Self::Failed => MessageKind::Failed,
        }
    }
}

/// What a message to an orchestrator says: a worker's report, or what the app noticed about a worker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    Done,
    Failed,
    /// The worker ended, or finished its turn, without reporting.
    NoReport,
    /// The worker is waiting for an answer; the user gives it in the worker's terminal.
    NeedsYou,
    /// A question one member asks another. It is kept with its sender and receiver, see `TeamExchange`.
    Question,
    /// The answer to a question, kept with its sender and receiver, see `TeamExchange`.
    Answer,
}

impl MessageKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Done => "done",
            Self::Failed => "failed",
            Self::NoReport => "noReport",
            Self::NeedsYou => "needsYou",
            Self::Question => "question",
            Self::Answer => "answer",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "done" => Some(Self::Done),
            "failed" => Some(Self::Failed),
            "noReport" => Some(Self::NoReport),
            "needsYou" => Some(Self::NeedsYou),
            "question" => Some(Self::Question),
            "answer" => Some(Self::Answer),
            _ => None,
        }
    }
}

/// A message to an orchestrator, about one of its members. It reaches the orchestrator once, at its next check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamMessage {
    pub id: String,
    pub member_id: String,
    pub kind: MessageKind,
    pub body: String,
    pub created_at: i64,
}

/// A question one member asks another, or the answer to it. Both members are kept, so the line says who asked
/// whom. It is filed under the asker, so the asker's orchestrator hears both at its next check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamExchange {
    pub id: String,
    /// `Question` or `Answer`; any other kind is refused when the line is kept.
    pub kind: MessageKind,
    pub from_member_id: String,
    pub to_member_id: String,
    pub body: String,
    pub created_at: i64,
}

/// The newest message kept for a member, as the screen shows it. The kind is the stored text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TeamLastMessage {
    pub kind: String,
    pub body: String,
    /// When the message was kept, in seconds since the Unix epoch. A u32: the contract has no 64-bit integers.
    pub created_at: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TeamMember {
    pub id: String,
    /// The absolute path of the worktree whose team this member is in. It stays when the session
    /// closes, so a session restored later goes back there.
    pub worktree: String,
    pub kind: TeamMemberKind,
    pub role: String,
    /// The agent's harness. Only agents have one, and a member has none until its first worker starts.
    pub harness: Option<HarnessId>,
    /// The Lynx bot. Only bots have one.
    pub bot_id: Option<String>,
    /// The orchestrator this member answers to. None means it is the root of a tree.
    pub parent_id: Option<String>,
    /// Whether it may have members under it. A bot always is.
    pub orchestrator: bool,
    /// Only a member with an orchestrator has a task and a "done when" line; otherwise both are empty.
    pub task: String,
    pub acceptance: String,
    /// Where the task stands. Only a member with a task has one.
    pub task_state: Option<TaskState>,
    /// The members whose tasks must be done before this one starts. All are in this member's worktree.
    pub depends_on: Vec<String>,
    /// Whether a worker of this member is running, or was stopped. None before its first worker starts.
    pub dispatch_state: Option<DispatchState>,
    /// The worktree the worker works in, when it is not the team's own: a worker started in a new worktree.
    pub execution_worktree: Option<String>,
    /// The terminal session the agent runs in. Only agents have one. It is kept after the session ends,
    /// because Resume finds the conversation through it.
    pub session_id: Option<String>,
    /// Whether the member's branch is open in the team tree. A new root starts open; every other new member
    /// starts closed.
    pub expanded: bool,
    /// The state of the worker this member is, when a worker of it has started. Not stored; the
    /// orchestration fills it from the member's reports and its process.
    pub worker_state: Option<String>,
    /// The newest message kept for this member. Only the snapshot fills it.
    pub last_message: Option<TeamLastMessage>,
    /// Whether the member's worker starts only once its team is released. Kept in the model; not sent to the screen.
    #[serde(skip)]
    pub start_when_released: bool,
}

/// A permission for two members to talk. Stored once, with the lower id first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TeamPair {
    pub a: String,
    pub b: String,
}

impl TeamPair {
    pub fn between(first: &str, second: &str) -> Self {
        if first <= second {
            Self {
                a: first.to_owned(),
                b: second.to_owned(),
            }
        } else {
            Self {
                a: second.to_owned(),
                b: first.to_owned(),
            }
        }
    }
}

/// A note a team shares, as `team_notes` keeps it: its worktree, its name and its text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TeamNote {
    /// The worktree whose team the note belongs to.
    pub worktree: String,
    pub name: String,
    pub body: String,
}

/// Every worktree's members, lines and notes, in the order they were made. The screen filters by worktree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TeamSnapshot {
    pub members: Vec<TeamMember>,
    pub pairs: Vec<TeamPair>,
    pub notes: Vec<TeamNote>,
}

/// A member to add, in the worktree it joins. An agent may start under an orchestrator of that worktree;
/// a bot is always an orchestrator and never under one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum NewTeamMember {
    Agent {
        role: String,
        /// None for a member that will be given a task and started later.
        harness: Option<HarnessId>,
        parent_id: Option<String>,
        orchestrator: bool,
        task: String,
        acceptance: String,
        /// The members this one waits for, in the same worktree.
        depends_on: Vec<String>,
        /// The absolute path of the worktree the agent works in.
        worktree: String,
        /// The terminal session the screen opened for the agent in `worktree`.
        session_id: Option<String>,
        /// A name for this member within one `add_members` batch, so another member of the batch can wait for it.
        /// Not sent to the screen.
        #[serde(skip)]
        key: Option<String>,
        /// Whether the worker starts only once the team is released. Not sent to the screen.
        #[serde(skip)]
        start_when_released: bool,
    },
    Bot {
        bot_id: String,
        /// The bot's name in the team.
        role: String,
        /// The absolute path of the worktree whose team the bot joins.
        worktree: String,
    },
}

/// The state a team card shows for a worker: its last report decides once it sent one, since an agent whose
/// turn ended can still have a process running. Without a report it runs only while its dispatch and process do.
pub fn worker_card_state(
    report: Option<Outcome>,
    dispatch: DispatchState,
    process_running: bool,
) -> &'static str {
    match report {
        Some(Outcome::Done) => "done",
        Some(Outcome::Failed) => "failed",
        None if dispatch == DispatchState::Running && process_running => "running",
        None => "stopped",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stored_state_reads_back_as_the_state_it_was_written_from() {
        for state in [
            TaskState::Pending,
            TaskState::Running,
            TaskState::Done,
            TaskState::Failed,
            TaskState::Cancelled,
        ] {
            assert_eq!(TaskState::parse(state.as_str()), Some(state));
        }
        for kind in [
            MessageKind::Done,
            MessageKind::Failed,
            MessageKind::NoReport,
            MessageKind::NeedsYou,
        ] {
            assert_eq!(MessageKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(TaskState::parse("waiting"), None);
    }

    #[test]
    fn a_report_decides_the_card_even_while_the_process_runs() {
        assert_eq!(
            worker_card_state(Some(Outcome::Done), DispatchState::Running, true),
            "done"
        );
        assert_eq!(
            worker_card_state(None, DispatchState::Running, true),
            "running"
        );
        assert_eq!(
            worker_card_state(None, DispatchState::Running, false),
            "stopped"
        );
        assert_eq!(
            worker_card_state(None, DispatchState::Stopped, true),
            "stopped"
        );
    }
}
