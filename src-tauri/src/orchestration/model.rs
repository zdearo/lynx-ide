//! The states of tasks and workers, and the rules that decide when a task may start.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::agents::harness::HarnessId;

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

/// What reaches the coordinator from its workers, or what the app noticed about them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum MessageKind {
    Done,
    Failed,
    /// The worker ended, or finished its turn, without reporting.
    NoReport,
    /// The worker is waiting for an answer; the user gives it in the worker's terminal.
    NeedsYou,
}

impl MessageKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Done => "done",
            Self::Failed => "failed",
            Self::NoReport => "no_report",
            Self::NeedsYou => "needs_you",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "done" => Some(Self::Done),
            "failed" => Some(Self::Failed),
            "no_report" => Some(Self::NoReport),
            "needs_you" => Some(Self::NeedsYou),
            _ => None,
        }
    }
}

/// Where a worker works: in a worktree of its own, or in the coordinator's, with its files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerWorktree {
    New,
    Current,
}

impl WorkerWorktree {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "new" => Some(Self::New),
            "current" => Some(Self::Current),
            _ => None,
        }
    }
}

/// Whether an agent reports through the Lynx tools. OMP has no MCP entry, so its end is read from
/// its status instead.
pub fn reports_through_tools(harness: HarnessId) -> bool {
    !matches!(harness, HarnessId::Omp)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub id: String,
    /// The coordinator's terminal session; the authority for everything the run does.
    pub coordinator_session: String,
    /// The worktree the coordinator works in; the workers' worktrees come from its repository.
    pub worktree_path: String,
    pub title: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub id: String,
    pub run_id: String,
    pub spec: String,
    /// Tasks of the same run that must be done before this one starts.
    pub depends_on: Vec<String>,
    pub state: TaskState,
    pub created_at: i64,
}

/// A worker: one agent started for one task, in a worktree of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dispatch {
    pub id: String,
    pub run_id: String,
    pub task_id: String,
    pub session_id: String,
    pub worktree_path: String,
    pub harness: HarnessId,
    pub state: DispatchState,
    pub started_at: i64,
}

/// A message for the coordinator: a worker's report, or what the app noticed about a worker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub id: String,
    pub run_id: String,
    pub dispatch_id: Option<String>,
    pub task_id: Option<String>,
    pub kind: MessageKind,
    pub body: String,
    pub created_at: i64,
}

/// Whether a task may start now: it is pending, and every task it depends on is done.
pub fn task_is_ready(task: &Task, tasks: &[Task]) -> bool {
    task.state == TaskState::Pending
        && task.depends_on.iter().all(|id| {
            tasks
                .iter()
                .any(|other| &other.id == id && other.state == TaskState::Done)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str, state: TaskState, depends_on: &[&str]) -> Task {
        Task {
            id: id.to_owned(),
            run_id: "run".to_owned(),
            spec: "do it".to_owned(),
            depends_on: depends_on.iter().map(|id| (*id).to_owned()).collect(),
            state,
            created_at: 0,
        }
    }

    #[test]
    fn a_task_starts_only_when_pending_and_its_dependencies_are_done() {
        let tasks = [
            task("a", TaskState::Done, &[]),
            task("b", TaskState::Running, &[]),
            task("c", TaskState::Pending, &["a"]),
            task("d", TaskState::Pending, &["b"]),
            task("e", TaskState::Pending, &["missing"]),
        ];
        assert!(task_is_ready(&tasks[2], &tasks));
        assert!(!task_is_ready(&tasks[3], &tasks));
        assert!(!task_is_ready(&tasks[4], &tasks));
        assert!(
            !task_is_ready(&tasks[0], &tasks),
            "a done task does not start again"
        );
    }

    #[test]
    fn a_failed_task_does_not_unblock_the_tasks_that_depend_on_it() {
        let tasks = [
            task("a", TaskState::Failed, &[]),
            task("b", TaskState::Pending, &["a"]),
        ];
        assert!(!task_is_ready(&tasks[1], &tasks));
    }

    #[test]
    fn states_round_trip_through_their_stored_text() {
        for state in [
            TaskState::Pending,
            TaskState::Running,
            TaskState::Done,
            TaskState::Failed,
            TaskState::Cancelled,
        ] {
            assert_eq!(TaskState::parse(state.as_str()), Some(state));
        }
        for state in [DispatchState::Running, DispatchState::Stopped] {
            assert_eq!(DispatchState::parse(state.as_str()), Some(state));
        }
        for kind in [
            MessageKind::Done,
            MessageKind::Failed,
            MessageKind::NoReport,
            MessageKind::NeedsYou,
        ] {
            assert_eq!(MessageKind::parse(kind.as_str()), Some(kind));
        }
    }

    #[test]
    fn a_worker_is_placed_in_a_new_worktree_or_the_current_one() {
        assert_eq!(WorkerWorktree::parse("new"), Some(WorkerWorktree::New));
        assert_eq!(
            WorkerWorktree::parse("current"),
            Some(WorkerWorktree::Current)
        );
        assert_eq!(WorkerWorktree::parse("elsewhere"), None);
    }

    #[test]
    fn omp_does_not_report_through_the_tools() {
        assert!(reports_through_tools(HarnessId::Claude));
        assert!(!reports_through_tools(HarnessId::Omp));
    }
}
