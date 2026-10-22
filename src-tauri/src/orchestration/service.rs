//! The orchestration rules: which run a session may use, when a task may start, and what a
//! coordinator hears back. Terminals and worktrees come through `WorkerHost`, so the rules are
//! tested without them.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use thiserror::Error;

use crate::agents::activity::AgentStatus;
use crate::agents::harness::HarnessId;
use crate::config::app_config::OrchestrationConfig;
use crate::persistence::{AppDatabase, DatabaseError};

use super::model::{
    Dispatch, DispatchState, Message, MessageKind, Outcome, Run, Task, TaskState, WorkerWorktree,
    reports_through_tools, task_is_ready,
};

const MAX_TITLE_CHARS: usize = 200;
const MAX_SPEC_CHARS: usize = 20_000;
const MAX_REPORT_CHARS: usize = 4_000;
const MAX_MODEL_CHARS: usize = 100;
/// How often a check looks again while it waits for something to arrive.
const CHECK_POLL: Duration = Duration::from_millis(500);

/// What the rules need from the rest of the app.
pub trait WorkerHost: Send + Sync + 'static {
    /// The orchestration settings as the user has them now.
    fn settings(&self) -> OrchestrationConfig;
    /// The worktree a terminal session works in, when the session belongs to a project.
    fn worktree_of(&self, session: &str) -> Option<String>;
    /// A new worktree called `name` in the repository `from` belongs to.
    fn new_worktree(&self, from: &str, name: &str) -> Result<String, String>;
    /// Writes `text` where an agent can read it, and returns the path.
    fn prompt_file(&self, text: &str) -> Result<String, String>;
    /// The line that starts an agent, with the launch mode and arguments Settings hold for it.
    fn launch_line(&self, harness: HarnessId) -> String;
    /// Starts a terminal in the worktree, with `command` typed into it. Returns the session.
    fn start(&self, worktree: &str, command: &str) -> Result<String, String>;
    /// Whether the session still runs.
    fn running(&self, session: &str) -> bool;
    /// The agent status of the session, when it reports one.
    fn status(&self, session: &str) -> Option<AgentStatus>;
    /// What the session printed lately, as text.
    fn output(&self, session: &str) -> Result<String, String>;
    /// Ends the session.
    fn stop(&self, session: &str);
}

/// Why an orchestration request is refused. The messages reach the agent, so they say what to do.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum OrchestrationError {
    #[error("orchestration is turned off in Lynx's settings, so this tool does nothing")]
    Disabled,
    #[error("this agent is not in a project worktree, so it cannot start a run")]
    NotInProject,
    #[error(
        "a worker cannot start runs, tasks or workers, or check on them; only the coordinator can"
    )]
    WorkerCannotCoordinate,
    #[error("only a worker reports to its coordinator")]
    NotAWorker,
    #[error("start a run with lynx_run_start first")]
    NoRun,
    #[error("this agent takes no model when Lynx starts it, so start it without one")]
    ModelNotSupported,
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
    #[error("the task already has its report: it is {0}")]
    AlreadyReported(String),
    #[error("this worker has been stopped, so it cannot report")]
    WorkerStopped,
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

/// What a coordinator reads about one worker: what it printed, and whether it still runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reading {
    pub output: String,
    pub running: bool,
    pub status: Option<AgentStatus>,
}

/// The state of each worker of a run, as a coordinator reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerState {
    pub dispatch_id: String,
    pub task_id: String,
    pub harness: HarnessId,
    pub task_state: TaskState,
    pub running: bool,
}

/// What a check returns: what happened since the last one, and where each worker stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub messages: Vec<Message>,
    pub workers: Vec<WorkerState>,
}

pub struct Orchestration {
    pub(super) database: Arc<AppDatabase>,
    host: Arc<dyn WorkerHost>,
    /// Which workers were waiting for an answer at the last look, so each wait is said once.
    waiting: Mutex<HashMap<String, bool>>,
}

impl Orchestration {
    pub fn new(database: Arc<AppDatabase>, host: Arc<dyn WorkerHost>) -> Self {
        Self {
            database,
            host,
            waiting: Mutex::new(HashMap::new()),
        }
    }

    /// A coordinator starts a run in its project.
    pub fn start_run(&self, coordinator: &str, title: &str) -> Result<Run, OrchestrationError> {
        self.ensure_enabled()?;
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
            created_at: now(),
        };
        self.database.insert_run(&run)?;
        Ok(run)
    }

    pub fn create_task(
        &self,
        coordinator: &str,
        spec: &str,
        depends_on: Vec<String>,
    ) -> Result<Task, OrchestrationError> {
        self.ensure_enabled()?;
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

    /// Starts an agent for a pending task, in a worktree of its own or in the coordinator's, as the
    /// coordinator chose. `model` is the one the user named, when there is one.
    pub fn start_worker(
        &self,
        coordinator: &str,
        task_id: &str,
        harness: HarnessId,
        worktree: WorkerWorktree,
        model: Option<&str>,
    ) -> Result<Dispatch, OrchestrationError> {
        self.ensure_enabled()?;
        let run = self.coordinator_run(coordinator)?;
        if let Some(model) = model {
            check_model(harness, model)?;
        }
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
        if !task_is_ready(&task, &tasks) {
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
            return Err(OrchestrationError::WaitingFor(waiting.join(", ")));
        }

        let worktree = match worktree {
            WorkerWorktree::Current => run.worktree_path.clone(),
            WorkerWorktree::New => {
                let name = format!("lynx-task-{}", task.id.chars().take(8).collect::<String>());
                self.host
                    .new_worktree(&run.worktree_path, &name)
                    .map_err(OrchestrationError::Host)?
            }
        };
        let prompt = self
            .host
            .prompt_file(&worker_prompt(harness, &task.spec))
            .map_err(OrchestrationError::Host)?;
        let line = self.host.launch_line(harness);
        let command = agent_command(harness, &line, &read_instruction(&prompt), model);
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

    /// A worker reports how its task ended, with a summary for its coordinator.
    pub fn report(
        &self,
        worker_session: &str,
        outcome: Outcome,
        summary: &str,
    ) -> Result<(), OrchestrationError> {
        self.ensure_enabled()?;
        let dispatch = self
            .database
            .dispatch_of_session(worker_session)?
            .ok_or(OrchestrationError::NotAWorker)?;
        if dispatch.state != DispatchState::Running {
            return Err(OrchestrationError::WorkerStopped);
        }
        let summary = summary.trim();
        if summary.is_empty() || summary.chars().count() > MAX_REPORT_CHARS {
            return Err(OrchestrationError::InvalidText(format!(
                "a report needs a summary of 1 to {MAX_REPORT_CHARS} characters"
            )));
        }
        let task = self
            .database
            .tasks_of_run(&dispatch.run_id)?
            .into_iter()
            .find(|task| task.id == dispatch.task_id)
            .ok_or_else(|| OrchestrationError::UnknownTask(dispatch.task_id.clone()))?;
        if task.state != TaskState::Running {
            return Err(OrchestrationError::AlreadyReported(
                task.state.as_str().to_owned(),
            ));
        }
        self.database
            .set_task_state(&task.id, outcome.task_state())?;
        self.database.insert_message(&Message {
            id: new_id(),
            run_id: dispatch.run_id,
            dispatch_id: Some(dispatch.id),
            task_id: Some(task.id),
            kind: outcome.message_kind(),
            body: summary.to_owned(),
            created_at: now(),
        })?;
        Ok(())
    }

    /// What a coordinator hears since its last check. Waits up to `wait` for a message. In the
    /// meantime it notices workers that ended without a report.
    pub fn check(&self, coordinator: &str, wait: Duration) -> Result<Check, OrchestrationError> {
        self.ensure_enabled()?;
        let run = self.coordinator_run(coordinator)?;
        let deadline = Instant::now() + wait;
        loop {
            self.notice_workers(&run)?;
            let messages = self.database.take_unread_messages(&run.id)?;
            if !messages.is_empty() || Instant::now() >= deadline {
                self.release_reported(&messages)?;
                return Ok(Check {
                    messages,
                    workers: self.worker_states(&run)?,
                });
            }
            std::thread::sleep(CHECK_POLL);
        }
    }

    /// What a worker of the coordinator's run printed lately, and its state.
    pub fn read_worker(
        &self,
        coordinator: &str,
        dispatch_id: &str,
    ) -> Result<(Dispatch, Reading), OrchestrationError> {
        self.ensure_enabled()?;
        let dispatch = self.worker_of_run(coordinator, dispatch_id)?;
        let output = match self.host.output(&dispatch.session_id) {
            Ok(text) => text,
            // A worker the app already closed has no screen left to read.
            Err(_) if dispatch.state == DispatchState::Stopped => String::new(),
            Err(error) => return Err(OrchestrationError::Host(error)),
        };
        let reading = Reading {
            output,
            running: self.host.running(&dispatch.session_id),
            status: self.host.status(&dispatch.session_id),
        };
        Ok((dispatch, reading))
    }

    /// Ends a worker. Its task is cancelled if it was still running.
    pub fn stop_worker(
        &self,
        coordinator: &str,
        dispatch_id: &str,
    ) -> Result<(), OrchestrationError> {
        self.ensure_enabled()?;
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

    /// Notices the workers that ended without a report, and for an agent that cannot report, the
    /// end of its turn. Each notice is written once.
    fn notice_workers(&self, run: &Run) -> Result<(), OrchestrationError> {
        let tasks = self.database.tasks_of_run(&run.id)?;
        for dispatch in self.database.dispatches_of_run(&run.id)? {
            if !is_active(&dispatch, &tasks) {
                continue;
            }
            let waiting = self.host.running(&dispatch.session_id)
                && self.host.status(&dispatch.session_id) == Some(AgentStatus::NeedsYou);
            let was_waiting = self
                .waiting
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(dispatch.id.clone(), waiting)
                .unwrap_or(false);
            if waiting && !was_waiting {
                self.database.insert_message(&Message {
                    id: new_id(),
                    run_id: dispatch.run_id.clone(),
                    dispatch_id: Some(dispatch.id.clone()),
                    task_id: Some(dispatch.task_id.clone()),
                    kind: MessageKind::NeedsYou,
                    body: "The worker is waiting for an answer. Read its output to see the question; the user gives the answer in the worker's terminal.".to_owned(),
                    created_at: now(),
                })?;
            }
            if !self.host.running(&dispatch.session_id) {
                self.database
                    .set_dispatch_state(&dispatch.id, DispatchState::Stopped)?;
                self.database
                    .set_task_state(&dispatch.task_id, TaskState::Failed)?;
                self.database.insert_message(&Message {
                    id: new_id(),
                    run_id: dispatch.run_id.clone(),
                    dispatch_id: Some(dispatch.id.clone()),
                    task_id: Some(dispatch.task_id.clone()),
                    kind: MessageKind::NoReport,
                    body:
                        "The worker ended without a report. Read its output to see how far it got."
                            .to_owned(),
                    created_at: now(),
                })?;
            } else if !reports_through_tools(dispatch.harness)
                && self.host.status(&dispatch.session_id) == Some(AgentStatus::Done)
                && !self
                    .database
                    .has_message(&dispatch.id, MessageKind::NoReport)?
            {
                // This agent cannot report, so the end of its turn is the most the app can see.
                self.database.insert_message(&Message {
                    id: new_id(),
                    run_id: dispatch.run_id.clone(),
                    dispatch_id: Some(dispatch.id.clone()),
                    task_id: Some(dispatch.task_id.clone()),
                    kind: MessageKind::NoReport,
                    body: "The worker finished its turn without a report. Read its output to see what it did.".to_owned(),
                    created_at: now(),
                })?;
            }
        }
        Ok(())
    }

    /// Closes the workers whose reports the coordinator has just heard, when Settings say so. The
    /// report stays with the coordinator; what the worker printed goes with its terminal.
    fn release_reported(&self, messages: &[Message]) -> Result<(), OrchestrationError> {
        if !self.host.settings().release_finished_workers {
            return Ok(());
        }
        for message in messages {
            if !matches!(message.kind, MessageKind::Done | MessageKind::Failed) {
                continue;
            }
            let Some(dispatch_id) = message.dispatch_id.as_deref() else {
                continue;
            };
            let Some(dispatch) = self.database.dispatch(dispatch_id)? else {
                continue;
            };
            if dispatch.state == DispatchState::Running {
                self.host.stop(&dispatch.session_id);
                self.database
                    .set_dispatch_state(&dispatch.id, DispatchState::Stopped)?;
            }
        }
        Ok(())
    }

    fn worker_states(&self, run: &Run) -> Result<Vec<WorkerState>, OrchestrationError> {
        let tasks = self.database.tasks_of_run(&run.id)?;
        let states = self
            .database
            .dispatches_of_run(&run.id)?
            .into_iter()
            .filter_map(|dispatch| {
                let task_state = tasks.iter().find(|task| task.id == dispatch.task_id)?.state;
                Some(WorkerState {
                    running: dispatch.state == DispatchState::Running
                        && self.host.running(&dispatch.session_id),
                    dispatch_id: dispatch.id,
                    task_id: dispatch.task_id,
                    harness: dispatch.harness,
                    task_state,
                })
            })
            .collect();
        Ok(states)
    }

    /// The coordinator's run is the newest one its session started.
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

    /// Every orchestration call is refused while the user has it turned off in Settings.
    fn ensure_enabled(&self) -> Result<(), OrchestrationError> {
        if self.host.settings().enabled {
            Ok(())
        } else {
            Err(OrchestrationError::Disabled)
        }
    }
}

/// A worker is active while it is not stopped and its task has no report yet. Its process may have
/// ended: that is what the checks notice.
fn is_active(dispatch: &Dispatch, tasks: &[Task]) -> bool {
    dispatch.state == DispatchState::Running
        && tasks
            .iter()
            .any(|task| task.id == dispatch.task_id && task.state == TaskState::Running)
}

/// What a worker reads: its task, and for an agent that reports through the tools, how to report.
fn worker_prompt(harness: HarnessId, spec: &str) -> String {
    if reports_through_tools(harness) {
        format!(
            "{spec}\n\nWhen you finish, or if you cannot, call the lynx_report tool with outcome \"done\" or \"failed\" and a short summary."
        )
    } else {
        spec.to_owned()
    }
}

/// The sentence that tells a worker to read its task from the file.
fn read_instruction(path: &str) -> String {
    format!("Read {path} and follow the instructions in it.")
}

/// A model name is typed into the shell with the rest of the line, so it is only the characters
/// that no shell reads as syntax. OpenCode's start command takes no model, so it is refused.
fn check_model(harness: HarnessId, model: &str) -> Result<(), OrchestrationError> {
    let plain = |c: char| c.is_ascii_alphanumeric() || "._/:@-".contains(c);
    if model.is_empty() || model.chars().count() > MAX_MODEL_CHARS || !model.chars().all(plain) {
        return Err(OrchestrationError::InvalidText(format!(
            "a model name is 1 to {MAX_MODEL_CHARS} letters, digits or . _ / : @ -"
        )));
    }
    if harness == HarnessId::Opencode {
        return Err(OrchestrationError::ModelNotSupported);
    }
    Ok(())
}

/// The line typed for a worker: its agent's launch line, the model, and the instruction. OpenCode
/// takes its first prompt through `--prompt`, the others take it after the line.
fn agent_command(harness: HarnessId, line: &str, instruction: &str, model: Option<&str>) -> String {
    let quoted = quote_for_shell(instruction, cfg!(windows));
    match harness {
        HarnessId::Opencode => format!("{line} --prompt {quoted}"),
        _ => {
            let model = model.map_or_else(String::new, |model| format!(" --model {model}"));
            format!("{line}{model} {quoted}")
        }
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
