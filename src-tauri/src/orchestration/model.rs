//! The states of a run, its tasks and its workers, and the rules that decide when one may move.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::agents::harness::HarnessId;
use crate::config::app_config::LaunchMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum RunState {
    /// Created by the coordinator; no worker starts until the user approves the budget.
    AwaitingApproval,
    Approved,
}

impl RunState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AwaitingApproval => "awaiting_approval",
            Self::Approved => "approved",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "awaiting_approval" => Some(Self::AwaitingApproval),
            "approved" => Some(Self::Approved),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum TaskState {
    Pending,
    Running,
    Done,
    Cancelled,
}

impl TaskState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "pending" => Some(Self::Pending),
            "running" => Some(Self::Running),
            "done" => Some(Self::Done),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
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

/// What the user approves once for a run. The coordinator can use less of it, never more.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Budget {
    /// Workers running at the same time.
    pub max_concurrent: u32,
    /// Workers the run may start in all.
    pub max_total: u32,
    /// The agents a worker may run.
    pub harnesses: Vec<HarnessId>,
    /// YOLO for the workers, only when the user approved it here.
    pub launch_mode: LaunchMode,
}

/// A run's budget must leave room for at least one worker, and name the agents it allows.
pub fn budget_is_valid(budget: &Budget) -> Result<(), &'static str> {
    if budget.max_concurrent == 0 || budget.max_total == 0 {
        return Err("the budget must allow at least one worker");
    }
    if budget.max_concurrent > budget.max_total {
        return Err("workers at the same time cannot be more than workers in all");
    }
    if budget.harnesses.is_empty() {
        return Err("the budget must allow at least one agent");
    }
    Ok(())
}

/// Why a worker may not start under a budget, given what the run already has running and started.
#[derive(Debug, PartialEq, Eq)]
pub enum BudgetRefusal {
    AgentNotAllowed,
    TooManyRunning,
    TooManyStarted,
}

pub fn budget_allows(
    budget: &Budget,
    running: u32,
    started: u32,
    harness: HarnessId,
) -> Result<(), BudgetRefusal> {
    if !budget.harnesses.contains(&harness) {
        return Err(BudgetRefusal::AgentNotAllowed);
    }
    if running >= budget.max_concurrent {
        return Err(BudgetRefusal::TooManyRunning);
    }
    if started >= budget.max_total {
        return Err(BudgetRefusal::TooManyStarted);
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub id: String,
    /// The coordinator's terminal session; the authority for everything the run does.
    pub coordinator_session: String,
    /// The worktree the coordinator works in; the workers' worktrees come from its repository.
    pub worktree_path: String,
    pub title: String,
    pub state: RunState,
    pub budget: Option<Budget>,
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

    fn budget() -> Budget {
        Budget {
            max_concurrent: 2,
            max_total: 3,
            harnesses: vec![HarnessId::Claude],
            launch_mode: LaunchMode::Manual,
        }
    }

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
    fn a_budget_needs_room_for_a_worker_and_an_agent() {
        assert!(budget_is_valid(&budget()).is_ok());
        assert!(
            budget_is_valid(&Budget {
                max_total: 0,
                ..budget()
            })
            .is_err()
        );
        assert!(
            budget_is_valid(&Budget {
                max_concurrent: 4,
                ..budget()
            })
            .is_err()
        );
        assert!(
            budget_is_valid(&Budget {
                harnesses: vec![],
                ..budget()
            })
            .is_err()
        );
    }

    #[test]
    fn a_worker_waits_for_its_slot_its_total_and_its_agent() {
        let b = budget();
        assert_eq!(budget_allows(&b, 0, 0, HarnessId::Claude), Ok(()));
        assert_eq!(
            budget_allows(&b, 0, 0, HarnessId::Codex),
            Err(BudgetRefusal::AgentNotAllowed)
        );
        assert_eq!(
            budget_allows(&b, 2, 2, HarnessId::Claude),
            Err(BudgetRefusal::TooManyRunning)
        );
        assert_eq!(
            budget_allows(&b, 0, 3, HarnessId::Claude),
            Err(BudgetRefusal::TooManyStarted)
        );
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
    fn states_round_trip_through_their_stored_text() {
        for state in [RunState::AwaitingApproval, RunState::Approved] {
            assert_eq!(RunState::parse(state.as_str()), Some(state));
        }
        for state in [
            TaskState::Pending,
            TaskState::Running,
            TaskState::Done,
            TaskState::Cancelled,
        ] {
            assert_eq!(TaskState::parse(state.as_str()), Some(state));
        }
        for state in [DispatchState::Running, DispatchState::Stopped] {
            assert_eq!(DispatchState::parse(state.as_str()), Some(state));
        }
    }
}
