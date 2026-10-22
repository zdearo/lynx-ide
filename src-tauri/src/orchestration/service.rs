//! The orchestration rules: which run a session may use, what a budget allows, and when a task may
//! start. Terminals and worktrees come through `WorkerHost`, so the rules are tested without them.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use thiserror::Error;

use crate::agents::activity::AgentStatus;
use crate::agents::harness::HarnessId;
use crate::config::app_config::LaunchMode;
use crate::persistence::{AppDatabase, DatabaseError};

use super::model::{
    Budget, BudgetRefusal, Dispatch, DispatchState, Run, RunState, Task, TaskState, budget_allows,
    budget_is_valid, task_is_ready,
};

const MAX_TITLE_CHARS: usize = 200;
const MAX_SPEC_CHARS: usize = 20_000;

/// What the rules need from the rest of the app.
pub trait WorkerHost: Send + Sync + 'static {
    /// The worktree a terminal session works in, when the session belongs to a project.
    fn worktree_of(&self, session: &str) -> Option<String>;
    /// A new worktree called `name` in the repository `from` belongs to.
    fn new_worktree(&self, from: &str, name: &str) -> Result<String, String>;
    /// Writes `text` where an agent can read it, and returns the path.
    fn prompt_file(&self, text: &str) -> Result<String, String>;
    /// The line that starts an agent in a mode, with the arguments Settings hold for it.
    fn launch_line(&self, harness: HarnessId, mode: LaunchMode) -> String;
    /// Starts a terminal in the worktree, with `command` typed into it. Returns the session.
    fn start(&self, worktree: &str, command: &str) -> Result<String, String>;
    /// Whether the session still runs.
    fn running(&self, session: &str) -> bool;
    /// What the session printed lately, and its agent status.
    fn reading(&self, session: &str) -> Result<Reading, String>;
    /// Ends the session.
    fn stop(&self, session: &str);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reading {
    pub output: String,
    pub running: bool,
    pub status: Option<AgentStatus>,
}

/// Why an orchestration request is refused. The messages reach the agent, so they say what to do.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum OrchestrationError {
    #[error("this agent is not in a project worktree, so it cannot start a run")]
    NotInProject,
    #[error("a worker cannot start runs, tasks or workers; only the coordinator can")]
    WorkerCannotCoordinate,
    #[error("start a run with lynx_run_start first")]
    NoRun,
    #[error("the user has not approved this run's budget yet")]
    NotApproved,
    #[error("this run already has an approved budget")]
    AlreadyApproved,
    #[error("no run {0}")]
    UnknownRun(String),
    #[error("{0}")]
    InvalidBudget(String),
    #[error("{0}")]
    InvalidText(String),
    #[error("no task {0} in this run")]
    UnknownTask(String),
    #[error("no worker {0} in this run")]
    UnknownWorker(String),
    #[error("the task is {0}, so it cannot start")]
    TaskNotPending(String),
    #[error("the task waits for tasks that are not done yet: {0}")]
    WaitingFor(String),
    #[error("the budget does not allow the agent {0}")]
    AgentNotAllowed(String),
    #[error("the budget allows {0} workers at the same time; wait for one to stop")]
    TooManyRunning(u32),
    #[error("the budget allows {0} workers in all, and the run has started them")]
    TooManyStarted(u32),
    #[error("{0}")]
    Host(String),
    #[error("the orchestration could not use the database: {0}")]
    Database(String),
}

impl From<DatabaseError> for OrchestrationError {
    fn from(error: DatabaseError) -> Self {
        Self::Database(error.to_string())
    }
}

pub struct Orchestration {
    database: Arc<AppDatabase>,
    host: Arc<dyn WorkerHost>,
}

impl Orchestration {
    pub fn new(database: Arc<AppDatabase>, host: Arc<dyn WorkerHost>) -> Self {
        Self { database, host }
    }

    /// A coordinator starts a run in its project. It waits for the user to approve the budget.
    pub fn start_run(&self, coordinator: &str, title: &str) -> Result<Run, OrchestrationError> {
        self.refuse_worker(coordinator)?;
        let worktree_path = self
            .host
            .worktree_of(coordinator)
            .ok_or(OrchestrationError::NotInProject)?;
        let title = title.trim();
        if title.is_empty() || title.chars().count() > MAX_TITLE_CHARS {
            return Err(OrchestrationError::InvalidText(format!(
                "the run needs a title of 1 to {MAX_TITLE_CHARS} characters"
            )));
        }
        let run = Run {
            id: new_id(),
            coordinator_session: coordinator.to_owned(),
            worktree_path,
            title: title.to_owned(),
            state: RunState::AwaitingApproval,
            budget: None,
            created_at: now(),
        };
        self.database.insert_run(&run)?;
        Ok(run)
    }

    /// The user approves a run's budget. Once approved, the budget does not change.
    pub fn approve(&self, run_id: &str, budget: Budget) -> Result<Run, OrchestrationError> {
        budget_is_valid(&budget)
            .map_err(|reason| OrchestrationError::InvalidBudget(reason.to_owned()))?;
        let run = self
            .database
            .run(run_id)?
            .ok_or_else(|| OrchestrationError::UnknownRun(run_id.to_owned()))?;
        if !self.database.approve_run(run_id, &budget)? {
            return Err(OrchestrationError::AlreadyApproved);
        }
        Ok(Run {
            state: RunState::Approved,
            budget: Some(budget),
            ..run
        })
    }

    pub fn create_task(
        &self,
        coordinator: &str,
        spec: &str,
        depends_on: Vec<String>,
    ) -> Result<Task, OrchestrationError> {
        let run = self.coordinator_run(coordinator)?;
        let spec = spec.trim();
        if spec.is_empty() || spec.chars().count() > MAX_SPEC_CHARS {
            return Err(OrchestrationError::InvalidText(format!(
                "a task needs a spec of 1 to {MAX_SPEC_CHARS} characters"
            )));
        }
        let known = self.database.tasks_of_run(&run.id)?;
        for dependency in &depends_on {
            if !known.iter().any(|task| &task.id == dependency) {
                return Err(OrchestrationError::UnknownTask(dependency.clone()));
            }
        }
        let task = Task {
            id: new_id(),
            run_id: run.id,
            spec: spec.to_owned(),
            depends_on,
            state: TaskState::Pending,
            created_at: now(),
        };
        self.database.insert_task(&task)?;
        Ok(task)
    }

    /// Starts an agent for a pending task, in a worktree of its own, if the budget and the task allow.
    pub fn start_worker(
        &self,
        coordinator: &str,
        task_id: &str,
        harness: HarnessId,
    ) -> Result<Dispatch, OrchestrationError> {
        let run = self.coordinator_run(coordinator)?;
        let budget = run.budget.clone().ok_or(OrchestrationError::NotApproved)?;
        let tasks = self.database.tasks_of_run(&run.id)?;
        let task = tasks
            .iter()
            .find(|task| task.id == task_id)
            .ok_or_else(|| OrchestrationError::UnknownTask(task_id.to_owned()))?
            .clone();
        if task.state != TaskState::Pending {
            return Err(OrchestrationError::TaskNotPending(
                task.state.as_str().to_owned(),
            ));
        }
        let waiting: Vec<&str> = task
            .depends_on
            .iter()
            .filter(|dependency| {
                !tasks
                    .iter()
                    .any(|other| &other.id == *dependency && other.state == TaskState::Done)
            })
            .map(String::as_str)
            .collect();
        if !task_is_ready(&task, &tasks) {
            return Err(OrchestrationError::WaitingFor(waiting.join(", ")));
        }

        let dispatches = self.database.dispatches_of_run(&run.id)?;
        let running = dispatches
            .iter()
            .filter(|dispatch| {
                dispatch.state == DispatchState::Running && self.host.running(&dispatch.session_id)
            })
            .count();
        let started = dispatches.len();
        budget_allows(&budget, count(running), count(started), harness).map_err(|refusal| {
            match refusal {
                BudgetRefusal::AgentNotAllowed => {
                    OrchestrationError::AgentNotAllowed(harness_label(harness))
                }
                BudgetRefusal::TooManyRunning => {
                    OrchestrationError::TooManyRunning(budget.max_concurrent)
                }
                BudgetRefusal::TooManyStarted => {
                    OrchestrationError::TooManyStarted(budget.max_total)
                }
            }
        })?;

        let name = format!("lynx-task-{}", task.id.chars().take(8).collect::<String>());
        let worktree = self
            .host
            .new_worktree(&run.worktree_path, &name)
            .map_err(OrchestrationError::Host)?;
        let prompt = self
            .host
            .prompt_file(&task.spec)
            .map_err(OrchestrationError::Host)?;
        let line = self.host.launch_line(harness, budget.launch_mode);
        let command = agent_command(harness, &line, &read_instruction(&prompt));
        let session = self
            .host
            .start(&worktree, &command)
            .map_err(OrchestrationError::Host)?;

        let dispatch = Dispatch {
            id: new_id(),
            run_id: run.id,
            task_id: task.id.clone(),
            session_id: session,
            worktree_path: worktree,
            harness,
            state: DispatchState::Running,
            started_at: now(),
        };
        self.database.insert_dispatch(&dispatch)?;
        self.database.set_task_state(&task.id, TaskState::Running)?;
        Ok(dispatch)
    }

    /// What a worker of the coordinator's run printed lately.
    pub fn read_worker(
        &self,
        coordinator: &str,
        dispatch_id: &str,
    ) -> Result<(Dispatch, Reading), OrchestrationError> {
        let dispatch = self.worker_of_run(coordinator, dispatch_id)?;
        let reading = self
            .host
            .reading(&dispatch.session_id)
            .map_err(OrchestrationError::Host)?;
        Ok((dispatch, reading))
    }

    /// Ends a worker. Its task is cancelled if it was still running.
    pub fn stop_worker(
        &self,
        coordinator: &str,
        dispatch_id: &str,
    ) -> Result<(), OrchestrationError> {
        let dispatch = self.worker_of_run(coordinator, dispatch_id)?;
        self.host.stop(&dispatch.session_id);
        self.database
            .set_dispatch_state(&dispatch.id, DispatchState::Stopped)?;
        let task = self
            .database
            .tasks_of_run(&dispatch.run_id)?
            .into_iter()
            .find(|task| task.id == dispatch.task_id);
        if let Some(task) = task.filter(|task| task.state == TaskState::Running) {
            self.database
                .set_task_state(&task.id, TaskState::Cancelled)?;
        }
        Ok(())
    }

    /// The run the coordinator session works in, the newest one it started.
    fn coordinator_run(&self, coordinator: &str) -> Result<Run, OrchestrationError> {
        self.refuse_worker(coordinator)?;
        self.database
            .latest_run_of(coordinator)?
            .ok_or(OrchestrationError::NoRun)
    }

    fn worker_of_run(
        &self,
        coordinator: &str,
        dispatch_id: &str,
    ) -> Result<Dispatch, OrchestrationError> {
        let run = self.coordinator_run(coordinator)?;
        self.database
            .dispatch(dispatch_id)?
            .filter(|dispatch| dispatch.run_id == run.id)
            .ok_or_else(|| OrchestrationError::UnknownWorker(dispatch_id.to_owned()))
    }

    fn refuse_worker(&self, session: &str) -> Result<(), OrchestrationError> {
        match self.database.dispatch_of_session(session)? {
            Some(_) => Err(OrchestrationError::WorkerCannotCoordinate),
            None => Ok(()),
        }
    }
}

fn count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn harness_label(harness: HarnessId) -> String {
    crate::agents::harness::harness_name(harness).to_owned()
}

/// The sentence that tells a worker to read its task from the file.
fn read_instruction(path: &str) -> String {
    format!("Read {path} and follow the instructions in it.")
}

/// The line typed for a worker: its agent's launch line and the instruction. OpenCode takes its
/// first prompt through `--prompt`, the others take it after the line.
fn agent_command(harness: HarnessId, line: &str, instruction: &str) -> String {
    let quoted = quote_for_shell(instruction, cfg!(windows));
    match harness {
        HarnessId::Opencode => format!("{line} --prompt {quoted}"),
        _ => format!("{line} {quoted}"),
    }
}

/// Quotes text as a shell would take it at a prompt: plain text as it is, anything else in quotes.
/// Mirrors `quotePathForShell` in `src/files/shell-quote.ts`.
pub fn quote_for_shell(text: &str, windows: bool) -> String {
    let plain = |c: char| {
        if windows {
            c.is_ascii_alphanumeric() || "_./:\\-".contains(c)
        } else {
            c.is_ascii_alphanumeric() || "_@%+=:,./-".contains(c)
        }
    };
    if !text.is_empty() && text.chars().all(plain) {
        return text.to_owned();
    }
    if windows {
        format!("\"{}\"", text.replace('"', "\\\""))
    } else {
        format!("'{}'", text.replace('\'', "'\\''"))
    }
}

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
        })
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};
    use std::sync::Mutex;

    use super::*;
    use crate::orchestration::model::TaskState;

    /// Terminals and worktrees in memory: a session belongs to a project when it is listed.
    #[derive(Default)]
    struct FakeHost {
        projects: HashMap<String, String>,
        running: Mutex<HashSet<String>>,
        started: Mutex<Vec<(String, String)>>,
        prompts: Mutex<Vec<String>>,
        output: Mutex<String>,
        count: Mutex<u32>,
    }

    impl FakeHost {
        fn with_project(session: &str) -> Self {
            let mut host = Self::default();
            host.projects
                .insert(session.to_owned(), "/repo/app".to_owned());
            host
        }

        fn started_commands(&self) -> Vec<(String, String)> {
            self.started.lock().expect("lock").clone()
        }
    }

    impl WorkerHost for FakeHost {
        fn worktree_of(&self, session: &str) -> Option<String> {
            self.projects.get(session).cloned()
        }

        fn new_worktree(&self, from: &str, name: &str) -> Result<String, String> {
            Ok(format!("{from}-worktrees/{name}"))
        }

        fn prompt_file(&self, text: &str) -> Result<String, String> {
            self.prompts.lock().expect("lock").push(text.to_owned());
            Ok("/tmp/lynx-prompts/task.md".to_owned())
        }

        fn launch_line(&self, harness: HarnessId, mode: LaunchMode) -> String {
            let command = crate::agents::harness::harness_name(harness);
            match mode {
                LaunchMode::Yolo => format!("{command} --yolo-flag"),
                LaunchMode::Manual => command.to_owned(),
            }
        }

        fn start(&self, worktree: &str, command: &str) -> Result<String, String> {
            let mut count = self.count.lock().expect("lock");
            *count += 1;
            let session = format!("worker-{count}");
            self.running.lock().expect("lock").insert(session.clone());
            self.started
                .lock()
                .expect("lock")
                .push((worktree.to_owned(), command.to_owned()));
            Ok(session)
        }

        fn running(&self, session: &str) -> bool {
            self.running.lock().expect("lock").contains(session)
        }

        fn reading(&self, _session: &str) -> Result<Reading, String> {
            Ok(Reading {
                output: self.output.lock().expect("lock").clone(),
                running: true,
                status: Some(AgentStatus::Working),
            })
        }

        fn stop(&self, session: &str) {
            self.running.lock().expect("lock").remove(session);
        }
    }

    fn budget(max_concurrent: u32, max_total: u32, mode: LaunchMode) -> Budget {
        Budget {
            max_concurrent,
            max_total,
            harnesses: vec![HarnessId::Claude, HarnessId::Codex],
            launch_mode: mode,
        }
    }

    /// A database in a temporary folder, with the coordinator's project session and a run started.
    fn setup(host: FakeHost) -> (Orchestration, Arc<FakeHost>, tempfile::TempDir, Run) {
        let dir = tempfile::tempdir().expect("temp dir");
        let database = Arc::new(AppDatabase::open(&dir.path().join("app.db")).expect("database"));
        let host = Arc::new(host);
        let orchestration = Orchestration::new(database, Arc::clone(&host) as Arc<dyn WorkerHost>);
        let run = orchestration
            .start_run("coordinator", "Audit")
            .expect("run");
        (orchestration, host, dir, run)
    }

    #[test]
    fn a_run_starts_in_the_coordinators_project_and_waits_for_approval() {
        let (orchestration, _host, _dir, run) = setup(FakeHost::with_project("coordinator"));
        assert_eq!(run.state, RunState::AwaitingApproval);
        assert_eq!(run.worktree_path, "/repo/app");
        assert_eq!(
            orchestration
                .create_task("coordinator", "x", vec![])
                .map(|t| t.state),
            Ok(TaskState::Pending),
            "tasks can be written before approval"
        );
    }

    #[test]
    fn a_session_outside_a_project_cannot_start_a_run() {
        let dir = tempfile::tempdir().expect("temp dir");
        let database = Arc::new(AppDatabase::open(&dir.path().join("app.db")).expect("database"));
        let orchestration = Orchestration::new(database, Arc::new(FakeHost::default()));
        assert_eq!(
            orchestration.start_run("loose", "Audit"),
            Err(OrchestrationError::NotInProject)
        );
    }

    #[test]
    fn no_worker_starts_before_the_user_approves_the_budget() {
        let (orchestration, _host, _dir, _run) = setup(FakeHost::with_project("coordinator"));
        let task = orchestration
            .create_task("coordinator", "fix it", vec![])
            .expect("task");
        assert_eq!(
            orchestration.start_worker("coordinator", &task.id, HarnessId::Claude),
            Err(OrchestrationError::NotApproved)
        );
    }

    #[test]
    fn an_approved_run_starts_a_worker_in_its_own_worktree_with_its_task() {
        let (orchestration, host, _dir, run) = setup(FakeHost::with_project("coordinator"));
        orchestration
            .approve(&run.id, budget(2, 3, LaunchMode::Manual))
            .expect("approve");
        let task = orchestration
            .create_task("coordinator", "fix the parser", vec![])
            .expect("task");

        let worker = orchestration
            .start_worker("coordinator", &task.id, HarnessId::Claude)
            .expect("start");

        assert_eq!(
            worker.worktree_path,
            format!("/repo/app-worktrees/lynx-task-{}", &task.id[..8])
        );
        assert_eq!(
            host.prompts.lock().expect("lock").as_slice(),
            ["fix the parser"]
        );
        let commands = host.started_commands();
        assert_eq!(commands.len(), 1);
        assert_eq!(
            commands[0].1,
            "claude 'Read /tmp/lynx-prompts/task.md and follow the instructions in it.'"
        );
    }

    #[test]
    fn yolo_reaches_a_worker_only_when_the_budget_approved_it() {
        let (orchestration, host, _dir, run) = setup(FakeHost::with_project("coordinator"));
        orchestration
            .approve(&run.id, budget(2, 3, LaunchMode::Manual))
            .expect("approve");
        let task = orchestration
            .create_task("coordinator", "x", vec![])
            .expect("task");
        orchestration
            .start_worker("coordinator", &task.id, HarnessId::Claude)
            .expect("start");
        assert!(!host.started_commands()[0].1.contains("--yolo-flag"));
    }

    #[test]
    fn a_task_waits_for_the_tasks_it_depends_on() {
        let (orchestration, _host, _dir, run) = setup(FakeHost::with_project("coordinator"));
        orchestration
            .approve(&run.id, budget(2, 3, LaunchMode::Manual))
            .expect("approve");
        let first = orchestration
            .create_task("coordinator", "first", vec![])
            .expect("task");
        let second = orchestration
            .create_task("coordinator", "second", vec![first.id.clone()])
            .expect("task");

        assert_eq!(
            orchestration.start_worker("coordinator", &second.id, HarnessId::Claude),
            Err(OrchestrationError::WaitingFor(first.id.clone()))
        );

        orchestration
            .database
            .set_task_state(&first.id, TaskState::Done)
            .expect("done");
        assert!(
            orchestration
                .start_worker("coordinator", &second.id, HarnessId::Claude)
                .is_ok()
        );
    }

    #[test]
    fn the_budget_caps_the_workers_running_at_once_and_in_all() {
        let (orchestration, _host, _dir, run) = setup(FakeHost::with_project("coordinator"));
        orchestration
            .approve(&run.id, budget(1, 2, LaunchMode::Manual))
            .expect("approve");
        let one = orchestration
            .create_task("coordinator", "one", vec![])
            .expect("task");
        let two = orchestration
            .create_task("coordinator", "two", vec![])
            .expect("task");
        let three = orchestration
            .create_task("coordinator", "three", vec![])
            .expect("task");

        let first = orchestration
            .start_worker("coordinator", &one.id, HarnessId::Claude)
            .expect("start");
        assert_eq!(
            orchestration.start_worker("coordinator", &two.id, HarnessId::Claude),
            Err(OrchestrationError::TooManyRunning(1)),
            "one worker runs, so a second waits"
        );

        orchestration
            .stop_worker("coordinator", &first.id)
            .expect("stop");
        let second = orchestration
            .start_worker("coordinator", &two.id, HarnessId::Claude)
            .expect("start");
        orchestration
            .stop_worker("coordinator", &second.id)
            .expect("stop");
        assert_eq!(
            orchestration.start_worker("coordinator", &three.id, HarnessId::Claude),
            Err(OrchestrationError::TooManyStarted(2)),
            "the run has started its two workers"
        );
    }

    #[test]
    fn the_budget_names_the_agents_a_worker_may_run() {
        let (orchestration, _host, _dir, run) = setup(FakeHost::with_project("coordinator"));
        orchestration
            .approve(&run.id, budget(2, 3, LaunchMode::Manual))
            .expect("approve");
        let task = orchestration
            .create_task("coordinator", "x", vec![])
            .expect("task");
        assert_eq!(
            orchestration.start_worker("coordinator", &task.id, HarnessId::Opencode),
            Err(OrchestrationError::AgentNotAllowed("opencode".to_owned()))
        );
    }

    #[test]
    fn a_worker_cannot_start_runs_tasks_or_workers() {
        let (orchestration, _host, _dir, run) = setup(FakeHost::with_project("coordinator"));
        orchestration
            .approve(&run.id, budget(2, 3, LaunchMode::Manual))
            .expect("approve");
        let task = orchestration
            .create_task("coordinator", "x", vec![])
            .expect("task");
        let worker = orchestration
            .start_worker("coordinator", &task.id, HarnessId::Claude)
            .expect("start");
        let worker_session = worker.session_id.clone();

        assert_eq!(
            orchestration.start_run(&worker_session, "nested"),
            Err(OrchestrationError::WorkerCannotCoordinate)
        );
        assert_eq!(
            orchestration.create_task(&worker_session, "nested", vec![]),
            Err(OrchestrationError::WorkerCannotCoordinate)
        );
        assert_eq!(
            orchestration.start_worker(&worker_session, &task.id, HarnessId::Claude),
            Err(OrchestrationError::WorkerCannotCoordinate)
        );
    }

    #[test]
    fn a_worker_can_be_read_and_stopped_by_its_coordinator_only() {
        let (orchestration, host, _dir, run) = setup(FakeHost::with_project("coordinator"));
        orchestration
            .approve(&run.id, budget(2, 3, LaunchMode::Manual))
            .expect("approve");
        let task = orchestration
            .create_task("coordinator", "x", vec![])
            .expect("task");
        let worker = orchestration
            .start_worker("coordinator", &task.id, HarnessId::Claude)
            .expect("start");
        *host.output.lock().expect("lock") = "hello from the worker".to_owned();

        let (_, reading) = orchestration
            .read_worker("coordinator", &worker.id)
            .expect("read");
        assert_eq!(reading.output, "hello from the worker");
        assert_eq!(reading.status, Some(AgentStatus::Working));
        assert_eq!(
            orchestration
                .read_worker("someone else", &worker.id)
                .map(|_| ()),
            Err(OrchestrationError::NoRun),
            "another session has no run to read from"
        );

        orchestration
            .stop_worker("coordinator", &worker.id)
            .expect("stop");
        assert!(!host.running(&worker.session_id));
        assert_eq!(
            orchestration.database.tasks_of_run(&run.id).expect("tasks")[0].state,
            TaskState::Cancelled
        );
    }

    #[test]
    fn a_budget_is_approved_once_and_must_be_usable() {
        let (orchestration, _host, _dir, run) = setup(FakeHost::with_project("coordinator"));
        assert!(matches!(
            orchestration.approve(&run.id, budget(0, 3, LaunchMode::Manual)),
            Err(OrchestrationError::InvalidBudget(_))
        ));
        orchestration
            .approve(&run.id, budget(2, 3, LaunchMode::Manual))
            .expect("approve");
        assert_eq!(
            orchestration.approve(&run.id, budget(1, 1, LaunchMode::Manual)),
            Err(OrchestrationError::AlreadyApproved)
        );
    }

    #[test]
    fn a_task_names_only_tasks_of_its_run_and_needs_a_spec() {
        let (orchestration, _host, _dir, _run) = setup(FakeHost::with_project("coordinator"));
        assert_eq!(
            orchestration
                .create_task("coordinator", "x", vec!["missing".to_owned()])
                .map(|_| ()),
            Err(OrchestrationError::UnknownTask("missing".to_owned()))
        );
        assert!(matches!(
            orchestration.create_task("coordinator", "   ", vec![]),
            Err(OrchestrationError::InvalidText(_))
        ));
    }

    #[test]
    fn quoting_follows_the_shell_of_each_platform() {
        assert_eq!(quote_for_shell("plain-path_1.md", false), "plain-path_1.md");
        assert_eq!(quote_for_shell("a b", false), "'a b'");
        assert_eq!(quote_for_shell("it's", false), "'it'\\''s'");
        assert_eq!(
            quote_for_shell("C:\\Users\\a b", true),
            "\"C:\\Users\\a b\""
        );
        assert_eq!(quote_for_shell("", false), "''");
    }
}
