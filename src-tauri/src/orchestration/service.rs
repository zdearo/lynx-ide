//! The coordination rules: who may add, start, read, answer or stop a worker, when a task may start, and what an
//! orchestrator hears back. The workers are members of the team, so the team holds what they do. Terminals and
//! worktrees come through `WorkerHost`, so the rules are tested without them.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, PoisonError, TryLockError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;

use crate::agents::activity::AgentStatus;
use crate::agents::harness::{HarnessId, harness_name};
use crate::agents::launch_line::unattended_settings;
use crate::bots::bot_bridge::bot_of_session;
use crate::config::app_config::{AgentEffort, OrchestrationConfig};
use crate::persistence::DatabaseError;
use crate::team::team_error::TeamError;
use crate::team::team_model::{
    DispatchState, MessageKind, NewTeamMember, Outcome, TaskState, TeamExchange, TeamMember,
    TeamMemberKind, TeamMessage, TeamSnapshot, worker_card_state,
};
use crate::team::team_rules::MAX_EXCHANGE_CHARS;
use crate::team::team_service::{TeamService, new_id};

use super::agent_wake::free_to_type;
use super::tools::AGENTS;

const MAX_REPORT_CHARS: usize = 4_000;
const MAX_MODEL_CHARS: usize = 100;
/// The longest answer a coordinator may type into a worker's terminal, in characters.
const MAX_REPLY: usize = 2000;
/// The longest question a member may send, in characters: a multi-line text pasted into a terminal.
const MAX_ASK: usize = 8_000;
/// How often a check looks again while it waits for something to arrive.
const CHECK_POLL: Duration = Duration::from_millis(500);
/// How often a question looks again while it waits for its turn, its typing or its answer.
const ASK_POLL: Duration = Duration::from_millis(100);
/// How long a worker may look idle after a question went in before its turn counts as over. An agent shows that it
/// started only after it reads the question, so an idle status that comes first is not the answer.
/// shortcut: a worker that never shows work counts as done after this, so its answer can come before its work ends.
/// Upgrade: end the turn only on the agent's Stop hook.
const IDLE_AFTER_ASK: Duration = Duration::from_secs(2);

/// Where a worker works: in a worktree of its own, or in the team's worktree, where the orchestrator works.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerWorktree {
    New,
    Current,
    /// The worktree the member's worker already works in: a replacement agent carries on there.
    Same,
}

impl WorkerWorktree {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "new" => Some(Self::New),
            "current" => Some(Self::Current),
            // `Same` is not something a caller names: only a replacement uses it.
            _ => None,
        }
    }
}

/// What the rules need from the rest of the app.
pub trait WorkerHost: Send + Sync + 'static {
    /// The orchestration settings as the user has them now.
    fn settings(&self) -> OrchestrationConfig;
    /// A new worktree called `name` in the repository `from` belongs to.
    fn new_worktree(&self, from: &str, name: &str) -> Result<String, String>;
    /// Writes `text` where an agent can read it, and returns the path.
    fn prompt_file(&self, text: &str) -> Result<String, String>;
    /// The line that starts an agent, with the launch mode and arguments Settings hold for it, and the effort the
    /// call names. `unattended` says nobody watches its terminal, so it must not stop to ask for permissions.
    fn launch_line(
        &self,
        harness: HarnessId,
        unattended: bool,
        effort: Option<AgentEffort>,
    ) -> String;
    /// Whether the coordinator may start workers in the project of `worktree`. Anyone may by default.
    /// A refusal says what to do, for the agent that reads it.
    fn allow_start(&self, _coordinator: &str, _worktree: &str) -> Result<(), String> {
        Ok(())
    }
    /// Whether the agent is on in Lynx's settings: its hooks and tools are in, so a worker can report back.
    fn agent_on(&self, _harness: HarnessId) -> bool {
        true
    }
    /// Types `text` into the session and presses Enter, to answer an agent that waits.
    fn send(&self, _session: &str, _text: &str) -> Result<(), String> {
        Err("this host cannot type into a terminal".to_owned())
    }
    /// Merges the branch of `from` into the branch checked out in `into`, and says what happened. A merge that
    /// does not go cleanly is undone, so nothing is left half-way.
    fn land(&self, _from: &str, _into: &str) -> Result<String, String> {
        Err("this host cannot merge worktrees".to_owned())
    }
    /// Starts a terminal in the worktree, with `command` typed into it. Returns the session.
    fn start(&self, worktree: &str, command: &str) -> Result<String, String>;
    /// Whether the session still runs.
    fn running(&self, session: &str) -> bool;
    /// The agent status of the session, when it reports one.
    fn status(&self, session: &str) -> Option<AgentStatus>;
    /// What the session printed lately, as text.
    fn output(&self, session: &str) -> Result<String, String>;
    /// Closes the session, so it leaves the terminal list the way a tab the person closed does.
    fn stop(&self, session: &str);
    /// A message was added for the orchestrator that started the worker in `session`, so it should look at its
    /// workers again. Only a bot is woken; an agent orchestrator checks when it chooses.
    fn notify(&self, _session: &str) {}
    /// The orchestrator read its workers now, so a note waiting for it is no longer needed. Nothing by default.
    fn checked(&self, _coordinator: &str) {}
    /// The team changed, so the screens that show it must be read again. Nothing by default.
    fn changed(&self) {}
    /// Sends Enter to a worker once `text` shows on its screen. Nothing by default.
    fn submit_when_shown(&self, _session: &str, _text: &str) {}
    /// Whether a question may be typed into the session now: the agent is idle or done, and the person has no
    /// draft in its input. The same guard that decides when a note is typed.
    fn ready_for_input(&self, session: &str) -> bool {
        free_to_type(self.status(session), false)
    }
    /// Types `text` into the session as a paste, then presses Enter, so a question with line breaks reads as one.
    fn send_pasted(&self, _session: &str, _text: &str) -> Result<(), String> {
        Err("this host cannot type into a terminal".to_owned())
    }
}

/// Why an orchestration request is refused. The messages reach the agent, so they say what to do.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum OrchestrationError {
    #[error("orchestration is turned off in Lynx's settings, so this tool does nothing")]
    Disabled,
    #[error(
        "this agent is not in a team, so it has no members to orchestrate: the person adds it in the Team tab"
    )]
    NotInTeam,
    #[error(
        "this agent is a member but not an orchestrator: the person makes it one in the Team tab"
    )]
    NotAnOrchestrator,
    #[error(
        "a worker cannot add members, start workers or check on them; only an orchestrator can"
    )]
    WorkerCannotCoordinate,
    #[error("only a worker reports to its orchestrator")]
    NotAWorker,
    #[error("no member {0} under you")]
    UnknownMember(String),
    #[error("several members are called {0}: use one of their ids: {1}")]
    AmbiguousMember(String, String),
    #[error("the member is still working on its task: talk to it with lynx_ask, or stop it first")]
    StillWorking,
    #[error("you orchestrate in several worktrees, so name the orchestrator with parentId: {0}")]
    ParentChoice(String),
    #[error("this agent takes no model when Lynx starts it, so start it without one")]
    ModelNotSupported,
    #[error("{0}")]
    InvalidText(String),
    #[error("the member has no task yet: give it one with lynx_member_add or in the Team tab")]
    NoTask,
    #[error("the task is {0}, so it cannot start")]
    TaskNotPending(String),
    #[error("the task waits for members that are not done yet: {0}")]
    WaitingFor(String),
    #[error("the task already has its report: it is {0}")]
    AlreadyReported(String),
    #[error(
        "{0} is off in Lynx's settings, so its workers could not report back: turn it on under Agents, then start the worker again"
    )]
    AgentOff(String),
    #[error("this worker has been stopped, so it cannot report")]
    WorkerStopped,
    #[error("the worker of {0} is still running: stop it before it leaves the team")]
    WorkerRunning(String),
    #[error("this member has not started a worker yet")]
    NoWorker,
    #[error(
        "the question was not typed and nothing was sent: the worker was busy, had another question first, or the person had a draft in its input. Ask again later, or read the worker with lynx_worker_read"
    )]
    AskNotTyped,
    #[error(
        "the question was typed into the worker, and it did not answer in time. Read the worker with lynx_worker_read to see how far it got"
    )]
    AskNoAnswer,
    #[error("{0} is not in your team: a member can only ask the members of its own team")]
    NotInYourTeam(String),
    #[error("a member cannot ask itself")]
    AskSelf,
    #[error(
        "that member already waits for an answer from you, directly or through others, so it cannot answer you: ask someone else, or wait until its answer to you is in"
    )]
    AskCycle,
    #[error("{0}")]
    Host(String),
    #[error("{0}")]
    Team(String),
    #[error("the orchestration could not use the database: {0}")]
    Database(String),
}

impl From<DatabaseError> for OrchestrationError {
    fn from(error: DatabaseError) -> Self {
        Self::Database(error.to_string())
    }
}

impl From<TeamError> for OrchestrationError {
    fn from(error: TeamError) -> Self {
        Self::Team(error.to_string())
    }
}

/// One worker of a bot's team, as the bot's agents tab shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkerRow {
    pub member_id: String,
    pub role: String,
    pub task: String,
    pub task_state: TaskState,
    pub harness: Option<HarnessId>,
    pub worktree_path: String,
    pub session_id: Option<String>,
    pub dispatch_state: DispatchState,
    pub running: bool,
    pub agent_status: Option<String>,
}

/// What a coordinator reads about one worker: its reports, what its terminal shows, and whether it still runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reading {
    /// The worker's own answers, oldest first. The screen is only what its terminal shows.
    pub reports: Vec<String>,
    pub output: String,
    pub running: bool,
    pub status: Option<AgentStatus>,
}

/// The state of one worker of an orchestrator's team, as the orchestrator reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerState {
    pub member_id: String,
    pub role: String,
    pub harness: Option<HarnessId>,
    pub task_state: TaskState,
    pub running: bool,
}

/// What a check returns: what happened since the last one, and where each worker stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub messages: Vec<TeamMessage>,
    pub workers: Vec<WorkerState>,
}

/// A worker that was just started: the member it runs for, its agent, its terminal and its worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerStart {
    pub member_id: String,
    pub harness: HarnessId,
    pub session_id: String,
    pub worktree_path: String,
}

/// What happened to a task given to a member that already had a worker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Assignment {
    /// The worker is still there and was sent the task.
    Sent,
    /// The worker is gone: the task waits for `lynx_worker_start`.
    Pending,
}

/// What a question to a worker returns: the report it sent for the question, if any, what its terminal shows, and
/// whether it still runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AskAnswer {
    pub report: Option<String>,
    pub output: String,
    pub running: bool,
    pub status: Option<AgentStatus>,
}

/// One member of an `add_members` call. `key` names it for the other members of the same call, which may wait for
/// it. `start` starts its worker with `agent`, now when what it waits for is done and once it is released otherwise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewMember {
    pub key: Option<String>,
    pub role: String,
    pub task: String,
    pub acceptance: String,
    pub depends_on: Vec<String>,
    pub start: bool,
    pub agent: Option<HarnessId>,
}

/// A member `add_members` wrote, and for a member that asked to start, what its start gave: the worker or the reason
/// it did not start. A member that waits for its dependencies has none yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddedMember {
    pub member: TeamMember,
    pub started: Option<Result<WorkerStart, String>>,
}

/// What a worker starts with besides its agent: the model and the effort the call names.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LaunchSetup {
    pub model: Option<String>,
    pub effort: Option<AgentEffort>,
}

pub struct Orchestration {
    team: Arc<TeamService>,
    host: Arc<dyn WorkerHost>,
    /// Which workers were waiting for an answer at the last look, so each wait is said once.
    waiting: Mutex<HashMap<String, bool>>,
    /// One gate per worker: a question holds it until its answer, so the next one waits its turn.
    asks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    /// The asks in progress, as (asker, asked), one entry per ask: two asks of the same pair can overlap, and each
    /// leaves its own entry. Kept in memory: an ask lasts one tool call.
    waits: Mutex<Vec<(String, String)>>,
}

impl Orchestration {
    pub fn new(team: Arc<TeamService>, host: Arc<dyn WorkerHost>) -> Self {
        Self {
            team,
            host,
            waiting: Mutex::new(HashMap::new()),
            asks: Mutex::new(HashMap::new()),
            waits: Mutex::new(Vec::new()),
        }
    }

    /// Adds members under one of the caller's orchestrators as one change: the whole list is checked before any member
    /// is written. A member may wait for another of the list by its key. A member that asks to start starts now when
    /// what it waits for is done; otherwise its agent is kept and its worker starts once it is released.
    pub fn add_members(
        &self,
        caller: &str,
        parent: Option<&str>,
        members: Vec<NewMember>,
    ) -> Result<Vec<AddedMember>, OrchestrationError> {
        if members.is_empty() {
            return Err(OrchestrationError::InvalidText(
                "a member list needs at least one member".to_owned(),
            ));
        }
        let parent = self.parent_under(caller, parent)?;
        for member in &members {
            if member.task.trim().is_empty() {
                return Err(OrchestrationError::InvalidText(
                    "a member needs a task: say what it must do".to_owned(),
                ));
            }
            if member.start {
                let agent = member.agent.ok_or_else(|| {
                    OrchestrationError::InvalidText(format!(
                        "a member that starts needs its agent: name one of {}",
                        AGENTS.join(", ")
                    ))
                })?;
                self.check_can_launch(caller, &parent.worktree, agent, None)?;
            }
        }
        let existing = self.team.members()?;
        let waits: Vec<bool> = members
            .iter()
            .map(|member| waits_for_tasks(&existing, &member.depends_on))
            .collect();
        let inputs = members
            .iter()
            .zip(&waits)
            .map(|(member, waits)| NewTeamMember::Agent {
                role: member.role.clone(),
                // A member that waits keeps its agent until it is released; one that starts now gets it at its start.
                harness: member.agent.filter(|_| member.start && *waits),
                parent_id: Some(parent.id.clone()),
                orchestrator: false,
                task: member.task.clone(),
                acceptance: member.acceptance.clone(),
                depends_on: member.depends_on.clone(),
                worktree: parent.worktree.clone(),
                session_id: None,
                key: member.key.clone(),
                start_when_released: member.start && *waits,
            })
            .collect();
        let written = self.team.add_members(inputs)?;
        self.host.changed();
        let mut added = Vec::with_capacity(written.len());
        for ((member, request), waits) in written.into_iter().zip(&members).zip(&waits) {
            let started = match request.agent {
                Some(agent) if request.start && !*waits => Some(
                    self.start_worker(
                        caller,
                        &member.id,
                        agent,
                        WorkerWorktree::Current,
                        LaunchSetup::default(),
                    )
                    .map_err(|error| error.to_string()),
                ),
                _ => None,
            };
            added.push(AddedMember { member, started });
        }
        // The members the calls above changed are read again, so the answer shows their state.
        let now = self.team.members()?;
        Ok(added
            .into_iter()
            .map(|added| AddedMember {
                member: now
                    .iter()
                    .find(|other| other.id == added.member.id)
                    .cloned()
                    .unwrap_or(added.member),
                started: added.started,
            })
            .collect())
    }

    /// The orchestrator the caller adds a member under: the one it names, or its only one.
    fn parent_under(
        &self,
        caller: &str,
        parent: Option<&str>,
    ) -> Result<TeamMember, OrchestrationError> {
        let parents = self.orchestrators(caller)?;
        let chosen = match (parent, parents.as_slice()) {
            (Some(id), _) => parents
                .iter()
                .find(|parent| parent.id == id)
                .ok_or_else(|| OrchestrationError::UnknownMember(id.to_owned()))?,
            (None, [only]) => only,
            (None, _) => {
                let choices: Vec<String> = parents
                    .iter()
                    .map(|parent| format!("{} ({})", parent.id, parent.worktree))
                    .collect();
                return Err(OrchestrationError::ParentChoice(choices.join(", ")));
            }
        };
        Ok(chosen.clone())
    }

    /// Starts the workers of the members that waited for finished tasks, once everything each waits for is done. A
    /// start that fails leaves its member pending, where the orchestrator sees it in the team.
    fn start_released(&self) -> Result<(), OrchestrationError> {
        let members = self.team.members()?;
        for member in members.iter().filter(|member| {
            member.start_when_released
                && member.task_state == Some(TaskState::Pending)
                && member.depends_on.iter().all(|id| {
                    members
                        .iter()
                        .any(|other| &other.id == id && other.task_state == Some(TaskState::Done))
                })
        }) {
            let caller = member
                .parent_id
                .as_deref()
                .and_then(|parent| members.iter().find(|other| other.id == parent))
                .and_then(|parent| parent.session_id.clone());
            let (Some(caller), Some(harness)) = (caller, member.harness) else {
                continue;
            };
            let _ = self.launch(
                &caller,
                member,
                harness,
                WorkerWorktree::Current,
                LaunchSetup::default(),
                false,
            );
        }
        Ok(())
    }

    /// One member under one of the caller's orchestrators, as `add_members` writes it with no start.
    #[cfg(test)]
    pub fn add_member(
        &self,
        caller: &str,
        parent: Option<&str>,
        role: &str,
        task: &str,
        acceptance: &str,
        depends_on: Vec<String>,
    ) -> Result<TeamMember, OrchestrationError> {
        let member = NewMember {
            key: None,
            role: role.to_owned(),
            task: task.to_owned(),
            acceptance: acceptance.to_owned(),
            depends_on,
            start: false,
            agent: None,
        };
        let mut added = self.add_members(caller, parent, vec![member])?;
        Ok(added.remove(0).member)
    }

    /// Removes one of the caller's members. A worker that still runs stops the dismissal, since nothing would
    /// hear its report any more.
    pub fn dismiss_member(&self, caller: &str, member_id: &str) -> Result<(), OrchestrationError> {
        let parents = self.orchestrators(caller)?;
        let member = self.child_of(&parents, member_id)?;
        let members = self.team.members()?;
        if let Some(running) = subtree(&members, &member.id)
            .into_iter()
            .find(|candidate| self.is_running(candidate))
        {
            return Err(OrchestrationError::WorkerRunning(running.role.clone()));
        }
        if let Some(session) = self.team.dismiss_member(&member.id)?
            && self.host.running(&session)
        {
            self.host.stop(&session);
        }
        self.host.changed();
        Ok(())
    }

    /// Starts an agent for one of the caller's members whose task is pending and whose waits are done. The
    /// worker works in a worktree of its own or in the team's, as the caller chose.
    pub fn start_worker(
        &self,
        caller: &str,
        member_ref: &str,
        harness: HarnessId,
        worktree: WorkerWorktree,
        setup: LaunchSetup,
    ) -> Result<WorkerStart, OrchestrationError> {
        let parents = self.orchestrators(caller)?;
        let member = self.child_of(&parents, member_ref)?;
        self.check_can_start(&member)?;
        self.launch(caller, &member, harness, worktree, setup, false)
    }

    /// The member's task is pending, has text, and everything it waits for is done.
    fn check_can_start(&self, member: &TeamMember) -> Result<(), OrchestrationError> {
        match member.task_state {
            Some(TaskState::Pending) => {}
            Some(state) => {
                return Err(OrchestrationError::TaskNotPending(
                    state.as_str().to_owned(),
                ));
            }
            None => return Err(OrchestrationError::NoTask),
        }
        if member.task.trim().is_empty() {
            return Err(OrchestrationError::NoTask);
        }
        let members = self.team.members()?;
        let waiting: Vec<String> = member
            .depends_on
            .iter()
            .filter_map(|id| members.iter().find(|other| &other.id == id))
            .filter(|other| other.task_state != Some(TaskState::Done))
            .map(|other| other.role.clone())
            .collect();
        if waiting.is_empty() {
            Ok(())
        } else {
            Err(OrchestrationError::WaitingFor(waiting.join(", ")))
        }
    }

    /// What can refuse an agent before anything is changed: the project's permission, the agent's hooks, the model.
    fn check_can_launch(
        &self,
        caller: &str,
        worktree: &str,
        harness: HarnessId,
        model: Option<&str>,
    ) -> Result<(), OrchestrationError> {
        self.host
            .allow_start(caller, worktree)
            .map_err(OrchestrationError::Host)?;
        // A worker reports through Lynx's tools, which come with the agent's hooks: without them it cannot finish.
        if !self.host.agent_on(harness) {
            return Err(OrchestrationError::AgentOff(
                harness_name(harness).to_owned(),
            ));
        }
        if let Some(model) = model {
            check_model(harness, model)?;
        }
        Ok(())
    }

    /// Opens a terminal for the member's worker and records it. `continuing` says another agent began the task.
    fn launch(
        &self,
        caller: &str,
        member: &TeamMember,
        harness: HarnessId,
        worktree: WorkerWorktree,
        setup: LaunchSetup,
        continuing: bool,
    ) -> Result<WorkerStart, OrchestrationError> {
        self.check_can_launch(caller, &member.worktree, harness, setup.model.as_deref())?;
        let project = member.worktree.clone();
        let execution = match worktree {
            WorkerWorktree::Current => project.clone(),
            WorkerWorktree::Same => member
                .execution_worktree
                .clone()
                .ok_or(OrchestrationError::NoWorker)?,
            WorkerWorktree::New => {
                let name = format!(
                    "lynx-task-{}",
                    member.id.chars().take(8).collect::<String>()
                );
                self.host
                    .new_worktree(&project, &name)
                    .map_err(OrchestrationError::Host)?
            }
        };
        let role = self.team.role_prompt(&member.role)?;
        let task = worker_task(member, continuing);
        let prompt = self
            .host
            .prompt_file(&worker_prompt(harness, role.as_deref(), &task))
            .map_err(OrchestrationError::Host)?;
        // A bot does not watch its workers' terminals, so they never stop to ask for permissions.
        let unattended = bot_of_session(caller).is_some();
        let line = self.host.launch_line(harness, unattended, setup.effort);
        // Nobody answers an unattended worker's first-run prompts, so its settings turn them off.
        let line = match unattended_settings(harness) {
            Some(settings) if unattended => format!(
                "{line} --settings {}",
                quote_for_shell(settings, cfg!(windows))
            ),
            _ => line,
        };
        let instruction = read_instruction(&prompt);
        let command = agent_command(harness, &line, &instruction, setup.model.as_deref());
        let session = self
            .host
            .start(&execution, &command)
            .map_err(OrchestrationError::Host)?;
        if let Err(error) = self
            .team
            .start_worker(&member.id, harness, &session, &execution, caller)
        {
            self.host.stop(&session);
            return Err(error.into());
        }
        if harness == HarnessId::Opencode {
            // OpenCode's --prompt only fills its input; the instruction is sent when Enter follows it.
            self.host.submit_when_shown(&session, &instruction);
        }
        self.host.changed();
        Ok(WorkerStart {
            member_id: member.id.clone(),
            harness,
            session_id: session,
            worktree_path: execution,
        })
    }

    /// Gives a member a new task. A worker that is still there takes it in its own session, with what it already
    /// knows; otherwise the task waits as pending for `lynx_worker_start`.
    pub fn assign(
        &self,
        caller: &str,
        member_ref: &str,
        task: &str,
        acceptance: &str,
    ) -> Result<Assignment, OrchestrationError> {
        let parents = self.orchestrators(caller)?;
        let member = self.child_of(&parents, member_ref)?;
        if task.trim().is_empty() {
            return Err(OrchestrationError::InvalidText(
                "say what the member must do".to_owned(),
            ));
        }
        if self.is_running(&member) && member.task_state == Some(TaskState::Running) {
            return Err(OrchestrationError::StillWorking);
        }
        let alive = self.is_running(&member);
        let new = TeamMember {
            task: task.trim().to_owned(),
            task_state: Some(TaskState::Pending),
            ..member.clone()
        };
        if !alive {
            self.check_can_start(&new)?;
            self.team
                .retask(&member.id, task, acceptance, TaskState::Pending)?;
            self.host.changed();
            return Ok(Assignment::Pending);
        }
        let (Some(session), Some(harness)) = (member.session_id.as_deref(), member.harness) else {
            return Err(OrchestrationError::NoWorker);
        };
        let task_for_worker = format!(
            "This is a new task for the same session, so what you learned stays with you.\n\n{}",
            worker_task(&new, false)
        );
        let role = self.team.role_prompt(&member.role)?;
        let prompt = self
            .host
            .prompt_file(&worker_prompt(harness, role.as_deref(), &task_for_worker))
            .map_err(OrchestrationError::Host)?;
        self.host
            .send(session, &read_instruction(&prompt))
            .map_err(OrchestrationError::Host)?;
        self.team
            .retask(&member.id, task, acceptance, TaskState::Running)?;
        self.team
            .set_dispatch_state(&member.id, DispatchState::Running)?;
        self.host.changed();
        Ok(Assignment::Sent)
    }

    /// Swaps the agent of a member's worker for another, in the same worktree and with the same task. The old
    /// agent's conversation does not carry over, so the new one is told to look at what was done.
    pub fn replace_worker(
        &self,
        caller: &str,
        member_ref: &str,
        harness: HarnessId,
        setup: LaunchSetup,
    ) -> Result<WorkerStart, OrchestrationError> {
        let parents = self.orchestrators(caller)?;
        let member = self.child_of(&parents, member_ref)?;
        if member.execution_worktree.is_none() {
            return Err(OrchestrationError::NoWorker);
        }
        // Refused before the old agent is touched, so a refusal never leaves the member without a worker.
        self.check_can_launch(caller, &member.worktree, harness, setup.model.as_deref())?;
        if let Some(session) = &member.session_id
            && self.host.running(session)
        {
            self.host.stop(session);
        }
        self.team
            .set_dispatch_state(&member.id, DispatchState::Stopped)?;
        self.team.set_task_state(&member.id, TaskState::Pending)?;
        self.launch(caller, &member, harness, WorkerWorktree::Same, setup, true)
    }

    /// Brings the work of a member that worked in a worktree of its own into the team's worktree. Only when the
    /// worker is stopped and both worktrees are clean; the host undoes a merge that does not go cleanly.
    pub fn land_worker(
        &self,
        caller: &str,
        member_ref: &str,
    ) -> Result<String, OrchestrationError> {
        let parents = self.orchestrators(caller)?;
        let member = self.child_of(&parents, member_ref)?;
        let from = member
            .execution_worktree
            .clone()
            .ok_or(OrchestrationError::NoWorker)?;
        if from == member.worktree {
            return Err(OrchestrationError::Host(
                "this worker worked in the team's own worktree, so there is nothing to land"
                    .to_owned(),
            ));
        }
        if self.is_running(&member) {
            return Err(OrchestrationError::WorkerRunning(member.role));
        }
        self.host
            .land(&from, &member.worktree)
            .map_err(OrchestrationError::Host)
    }

    /// The roles an orchestrator can give its members: a name and the prompt a worker of that role reads.
    pub fn roles(&self, caller: &str) -> Result<Vec<(String, String)>, OrchestrationError> {
        self.orchestrators(caller)?;
        Ok(self.team.roles()?)
    }

    /// Creates a role, or replaces the prompt of the one with that name. A member with that role reads it.
    pub fn save_role(
        &self,
        caller: &str,
        name: &str,
        prompt: &str,
    ) -> Result<(), OrchestrationError> {
        self.orchestrators(caller)?;
        Ok(self.team.save_role(name, prompt)?)
    }

    pub fn delete_role(&self, caller: &str, name: &str) -> Result<bool, OrchestrationError> {
        self.orchestrators(caller)?;
        Ok(self.team.delete_role(name)?)
    }

    /// The worktree whose notes a call is about: the caller's own, or the one it names when it is in several.
    pub fn notes_worktree(
        &self,
        caller: &str,
        named: Option<&str>,
    ) -> Result<String, OrchestrationError> {
        self.ensure_enabled()?;
        let mut trees: Vec<String> = self
            .caller_members(caller)?
            .into_iter()
            .map(|member| member.worktree)
            .collect();
        trees.sort();
        trees.dedup();
        match (named, trees.as_slice()) {
            (_, []) => Err(OrchestrationError::NotInTeam),
            (Some(wanted), _) if trees.iter().any(|tree| tree == wanted) => Ok(wanted.to_owned()),
            (Some(wanted), _) => Err(OrchestrationError::InvalidText(format!(
                "you are not in a team of {wanted}"
            ))),
            (None, [only]) => Ok(only.clone()),
            (None, _) => Err(OrchestrationError::InvalidText(format!(
                "you are in several worktrees, so name one with worktree: {}",
                trees.join(", ")
            ))),
        }
    }

    pub fn note_names(&self, worktree: &str) -> Result<Vec<String>, OrchestrationError> {
        Ok(self.team.note_names(worktree)?)
    }

    pub fn read_note(&self, worktree: &str, name: &str) -> Result<String, OrchestrationError> {
        Ok(self.team.note(worktree, name)?)
    }

    pub fn write_note(
        &self,
        worktree: &str,
        name: &str,
        content: &str,
    ) -> Result<(), OrchestrationError> {
        Ok(self.team.write_note(worktree, name, content)?)
    }

    pub fn edit_note(
        &self,
        worktree: &str,
        name: &str,
        old: &str,
        new: &str,
    ) -> Result<(), OrchestrationError> {
        Ok(self.team.edit_note(worktree, name, old, new)?)
    }

    /// Deleting is for an orchestrator: a worker writes notes but never removes the team's.
    pub fn delete_note(
        &self,
        caller: &str,
        worktree: &str,
        name: &str,
    ) -> Result<(), OrchestrationError> {
        self.orchestrators(caller)?;
        Ok(self.team.delete_note(worktree, name)?)
    }

    /// A worker reports how its task ended, with a summary for its orchestrator.
    pub fn report(
        &self,
        worker_session: &str,
        outcome: Outcome,
        summary: &str,
    ) -> Result<(), OrchestrationError> {
        self.ensure_enabled()?;
        let worker = self
            .team
            .members()?
            .into_iter()
            .find(|member| {
                member.session_id.as_deref() == Some(worker_session)
                    && member.dispatch_state.is_some()
            })
            .ok_or(OrchestrationError::NotAWorker)?;
        if worker.dispatch_state != Some(DispatchState::Running) {
            return Err(OrchestrationError::WorkerStopped);
        }
        let summary = summary.trim();
        if summary.is_empty() || summary.chars().count() > MAX_REPORT_CHARS {
            return Err(OrchestrationError::InvalidText(format!(
                "a report needs a summary of 1 to {MAX_REPORT_CHARS} characters"
            )));
        }
        if worker.task_state != Some(TaskState::Running) {
            return Err(OrchestrationError::AlreadyReported(
                worker
                    .task_state
                    .map_or("missing", TaskState::as_str)
                    .to_owned(),
            ));
        }
        let message = TeamMessage {
            id: new_id(),
            member_id: worker.id.clone(),
            kind: outcome.message_kind(),
            body: summary.to_owned(),
            created_at: now(),
        };
        self.team
            .record_report(&worker.id, outcome.task_state(), &message)?;
        // A member that waited for this task may start now; a start that fails leaves it pending.
        self.start_released()?;
        // The worker's card shows Done from this report, even while its process still runs.
        self.host.changed();
        self.tell(&worker)?;
        Ok(())
    }

    /// The members the caller is, as a bot or as an agent, and the orchestrators among them. A worker cannot
    /// orchestrate, and an agent that is not an orchestrator cannot either.
    fn orchestrators(&self, caller: &str) -> Result<Vec<TeamMember>, OrchestrationError> {
        self.ensure_enabled()?;
        let mine = self.caller_members(caller)?;
        if mine.iter().any(|member| member.dispatch_state.is_some()) {
            return Err(OrchestrationError::WorkerCannotCoordinate);
        }
        if mine.is_empty() {
            return Err(OrchestrationError::NotInTeam);
        }
        let orchestrators: Vec<TeamMember> = mine
            .into_iter()
            .filter(|member| member.orchestrator)
            .collect();
        if orchestrators.is_empty() {
            return Err(OrchestrationError::NotAnOrchestrator);
        }
        Ok(orchestrators)
    }

    /// Every member the caller stands for: a bot's member in each team it is in, or the agent's own member.
    fn caller_members(&self, caller: &str) -> Result<Vec<TeamMember>, OrchestrationError> {
        let bot = bot_of_session(caller);
        Ok(self
            .team
            .members()?
            .into_iter()
            .filter(|member| match bot {
                Some(bot) => {
                    member.kind == TeamMemberKind::Bot && member.bot_id.as_deref() == Some(bot)
                }
                None => {
                    member.kind == TeamMemberKind::Agent
                        && member.session_id.as_deref() == Some(caller)
                }
            })
            .collect())
    }

    /// The member `member_ref` names, by id or by role, when it answers to one of the orchestrators. Nothing else is
    /// the caller's to manage. A role shared by several members must be told apart by id.
    fn child_of(
        &self,
        parents: &[TeamMember],
        member_ref: &str,
    ) -> Result<TeamMember, OrchestrationError> {
        let children: Vec<TeamMember> = self
            .team
            .members()?
            .into_iter()
            .filter(|member| {
                member
                    .parent_id
                    .as_ref()
                    .is_some_and(|parent| parents.iter().any(|mine| &mine.id == parent))
            })
            .collect();
        if let Some(member) = children.iter().find(|member| member.id == member_ref) {
            return Ok(member.clone());
        }
        let wanted = member_ref.trim();
        let named: Vec<&TeamMember> = children
            .iter()
            .filter(|member| member.role.eq_ignore_ascii_case(wanted))
            .collect();
        match named.as_slice() {
            [only] => Ok((*only).clone()),
            [] => Err(OrchestrationError::UnknownMember(member_ref.to_owned())),
            several => Err(OrchestrationError::AmbiguousMember(
                wanted.to_owned(),
                several
                    .iter()
                    .map(|member| member.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
            )),
        }
    }

    /// Whether a member's worker is running now: its dispatch runs and its process is alive.
    fn is_running(&self, member: &TeamMember) -> bool {
        member.dispatch_state == Some(DispatchState::Running)
            && member
                .session_id
                .as_deref()
                .is_some_and(|session| self.host.running(session))
    }

    /// Tells the orchestrator that started a worker that a message was added. Only a bot is woken, through its
    /// chat; an agent is woken through its terminal.
    fn tell(&self, member: &TeamMember) -> Result<(), OrchestrationError> {
        if let Some(session) = self.team.notify_session_of(&member.id)? {
            self.host.notify(&session);
        }
        Ok(())
    }

    /// Adds a message for an orchestrator about one of its workers, and tells it.
    fn note(
        &self,
        worker: &TeamMember,
        kind: MessageKind,
        body: &str,
    ) -> Result<(), OrchestrationError> {
        self.team.add_message(&TeamMessage {
            id: new_id(),
            member_id: worker.id.clone(),
            kind,
            body: body.to_owned(),
            created_at: now(),
        })?;
        self.host.changed();
        self.tell(worker)
    }

    /// Forgets nothing when a bot is deleted: its members go with it in the team. Stops every worker of the bot
    /// that still runs, first, so nothing keeps working for a bot that is gone.
    pub fn stop_workers_of_bot(&self, bot: &str) -> Result<usize, OrchestrationError> {
        let members = self.team.members()?;
        let owners = bot_member_ids(&members, bot);
        let running: Vec<String> = members
            .iter()
            .filter(|member| {
                member.dispatch_state == Some(DispatchState::Running)
                    && member
                        .parent_id
                        .as_deref()
                        .is_some_and(|parent| owners.contains(&parent))
            })
            .filter_map(|member| member.session_id.clone())
            .collect();
        for session in &running {
            self.host.stop(session);
        }
        Ok(running.len())
    }

    /// The workers of every member a bot has, in every worktree: the members that have started a worker.
    pub fn workers_of_bot(&self, bot: &str) -> Result<Vec<WorkerRow>, OrchestrationError> {
        let members = self.team.members()?;
        let owners = bot_member_ids(&members, bot);
        let mut rows = Vec::new();
        for member in members.iter().rev() {
            let Some(dispatch_state) = member.dispatch_state else {
                continue;
            };
            if !member
                .parent_id
                .as_deref()
                .is_some_and(|parent| owners.contains(&parent))
            {
                continue;
            }
            rows.push(WorkerRow {
                member_id: member.id.clone(),
                role: member.role.clone(),
                task: member.task.clone(),
                task_state: member.task_state.unwrap_or(TaskState::Pending),
                harness: member.harness,
                worktree_path: member
                    .execution_worktree
                    .clone()
                    .unwrap_or_else(|| member.worktree.clone()),
                session_id: member.session_id.clone(),
                dispatch_state,
                running: self.is_running(member),
                agent_status: member
                    .session_id
                    .as_deref()
                    .and_then(|session| self.host.status(session))
                    .map(agent_status_name),
            });
        }
        Ok(rows)
    }

    /// Fills each worker member's state on a team snapshot. Members that never started a worker have none.
    pub fn fill_worker_states(
        &self,
        snapshot: &mut TeamSnapshot,
    ) -> Result<(), OrchestrationError> {
        self.fill_states(&mut snapshot.members)
    }

    fn fill_states(&self, members: &mut [TeamMember]) -> Result<(), OrchestrationError> {
        for member in members {
            let Some(dispatch_state) = member.dispatch_state else {
                member.worker_state = None;
                continue;
            };
            let report = self
                .team
                .messages_of(&member.id)?
                .iter()
                .rev()
                .find_map(|message| match message.kind {
                    MessageKind::Done => Some(Outcome::Done),
                    MessageKind::Failed => Some(Outcome::Failed),
                    MessageKind::NoReport
                    | MessageKind::NeedsYou
                    | MessageKind::Question
                    | MessageKind::Answer => None,
                });
            let process_running = member
                .session_id
                .as_deref()
                .is_some_and(|session| self.host.running(session));
            member.worker_state =
                Some(worker_card_state(report, dispatch_state, process_running).to_owned());
        }
        Ok(())
    }

    /// The team the caller is in, in every worktree the caller is in: the whole tree, with each worker's state.
    pub fn team_view(&self, caller: &str) -> Result<Vec<TeamMember>, OrchestrationError> {
        self.ensure_enabled()?;
        let mine = self.caller_members(caller)?;
        if mine.is_empty() {
            return Err(OrchestrationError::NotInTeam);
        }
        let mut members: Vec<TeamMember> = self
            .team
            .members()?
            .into_iter()
            .filter(|member| mine.iter().any(|own| own.worktree == member.worktree))
            .collect();
        self.fill_states(&mut members)?;
        Ok(members)
    }

    /// What an orchestrator hears since its last check. Waits up to `wait` for a message. In the meantime it notices
    /// workers that ended without a report.
    pub fn check(&self, caller: &str, wait: Duration) -> Result<Check, OrchestrationError> {
        let parents = self.orchestrators(caller)?;
        let parent_ids: Vec<String> = parents.iter().map(|parent| parent.id.clone()).collect();
        self.host.checked(caller);
        let deadline = Instant::now() + wait;
        loop {
            self.notice_workers(&parent_ids)?;
            let messages = self.team.take_unread_messages(&parent_ids)?;
            if !messages.is_empty() || Instant::now() >= deadline {
                self.release_reported(&messages)?;
                return Ok(Check {
                    messages,
                    workers: self.worker_states(&parent_ids)?,
                });
            }
            std::thread::sleep(CHECK_POLL);
        }
    }

    /// What a worker of the caller's team reported and printed lately, and its state.
    pub fn read_worker(
        &self,
        caller: &str,
        member_id: &str,
    ) -> Result<(TeamMember, Reading), OrchestrationError> {
        let parents = self.orchestrators(caller)?;
        let member = self.child_of(&parents, member_id)?;
        let session = member
            .session_id
            .clone()
            .ok_or(OrchestrationError::NoWorker)?;
        let output = self.screen_of(&session)?;
        let reports = self
            .team
            .messages_of(&member.id)?
            .into_iter()
            .filter(|message| matches!(message.kind, MessageKind::Done | MessageKind::Failed))
            .map(|message| message.body)
            .collect();
        let reading = Reading {
            reports,
            output,
            running: self.host.running(&session),
            status: self.host.status(&session),
        };
        Ok((member, reading))
    }

    /// Answers a worker that waits: `text` is typed into its terminal and sent. One line, no control
    /// characters, so an answer cannot be anything but an answer.
    pub fn reply_worker(
        &self,
        caller: &str,
        member_id: &str,
        text: &str,
    ) -> Result<(), OrchestrationError> {
        let parents = self.orchestrators(caller)?;
        let member = self.child_of(&parents, member_id)?;
        let session = member
            .session_id
            .clone()
            .ok_or(OrchestrationError::NoWorker)?;
        let text = text.trim();
        if text.is_empty() || text.chars().count() > MAX_REPLY || text.chars().any(char::is_control)
        {
            return Err(OrchestrationError::InvalidText(format!(
                "an answer is one line of 1 to {MAX_REPLY} characters, with no control characters"
            )));
        }
        if !self.host.running(&session) {
            return Err(OrchestrationError::Host(
                "the worker has ended, so there is no one to answer".to_owned(),
            ));
        }
        self.host
            .send(&session, text)
            .map_err(OrchestrationError::Host)
    }

    /// Types `text` into a running worker and waits for its answer, within `wait` in all. The question waits behind
    /// the worker's other questions, and while the worker is busy or the person has a draft in its input. The answer
    /// is the report the worker sends after the question, or the end of its turn, with what its terminal shows. The
    /// question and the answer are kept in the team, both read, since the caller has them already.
    pub fn ask_worker(
        &self,
        caller: &str,
        member_ref: &str,
        text: &str,
        wait: Duration,
    ) -> Result<AskAnswer, OrchestrationError> {
        let parents = self.orchestrators(caller)?;
        let member = self.child_of(&parents, member_ref)?;
        let session = member
            .session_id
            .clone()
            .ok_or(OrchestrationError::NoWorker)?;
        let from = member
            .parent_id
            .clone()
            .ok_or_else(|| OrchestrationError::UnknownMember(member_ref.to_owned()))?;
        let text = clean_question(text)?;
        if !self.host.running(&session) {
            return Err(OrchestrationError::Host(
                "the worker has ended, so there is no one to answer".to_owned(),
            ));
        }
        let _asking = self.start_ask(&from, &member.id)?;
        let deadline = Instant::now() + wait;
        let report = self.ask_session(&member, &session, &text, deadline, || {
            self.record(MessageKind::Question, &from, &member.id, &text, true)
        })?;
        // The report goes to the caller now, so the next check does not say it again.
        let report = match report {
            Some(message) => {
                self.team.mark_message_read(&message.id)?;
                Some(message.body)
            }
            None => None,
        };
        let output = self.screen_of(&session)?;
        self.record(
            MessageKind::Answer,
            &member.id,
            &from,
            &answer_body(report.as_deref(), &output),
            true,
        )?;
        self.host.checked(caller);
        Ok(AskAnswer {
            report,
            output,
            running: self.host.running(&session),
            status: self.host.status(&session),
        })
    }

    /// A member of the caller's team asks another member a question and waits for its answer, as `ask_worker` does
    /// for an orchestrator. The target is named by id or by role and must be in the same team. A member that already
    /// waits for the asker, directly or through others, cannot answer, so the question is refused at once. The
    /// question and the answer stay unread in the team: the asker's orchestrator hears them at its next check.
    pub fn member_ask(
        &self,
        caller: &str,
        member_ref: &str,
        text: &str,
        wait: Duration,
    ) -> Result<AskAnswer, OrchestrationError> {
        self.ensure_enabled()?;
        let asker = self
            .caller_members(caller)?
            .into_iter()
            .find(|member| member.kind == TeamMemberKind::Agent)
            .ok_or(OrchestrationError::NotInTeam)?;
        let target = self.teammate(&asker.worktree, member_ref)?;
        let session = target
            .session_id
            .clone()
            .ok_or(OrchestrationError::NoWorker)?;
        let text = clean_question(text)?;
        if !self.host.running(&session) {
            return Err(OrchestrationError::Host(
                "the worker has ended, so there is no one to answer".to_owned(),
            ));
        }
        let _asking = self.start_ask(&asker.id, &target.id)?;
        let deadline = Instant::now() + wait;
        let report = self.ask_session(&target, &session, &text, deadline, || {
            self.record(MessageKind::Question, &asker.id, &target.id, &text, false)
        })?;
        let output = self.screen_of(&session)?;
        let report = report.map(|message| message.body);
        self.record(
            MessageKind::Answer,
            &target.id,
            &asker.id,
            &answer_body(report.as_deref(), &output),
            false,
        )?;
        Ok(AskAnswer {
            report,
            output,
            running: self.host.running(&session),
            status: self.host.status(&session),
        })
    }

    /// Waits for the worker's turn, types `text` into it and waits for the answer, all within `deadline`. `on_typed`
    /// runs once the question is in the worker's input, before it is sent. The answer is the report the worker sent
    /// after the question, if one came; None when its turn ended without one.
    fn ask_session(
        &self,
        member: &TeamMember,
        session: &str,
        text: &str,
        deadline: Instant,
        on_typed: impl FnOnce() -> Result<(), OrchestrationError>,
    ) -> Result<Option<TeamMessage>, OrchestrationError> {
        let gate = self.ask_gate(&member.id);
        let _turn = poll_until(deadline, || {
            Ok(match gate.try_lock() {
                Ok(turn) => Some(turn),
                Err(TryLockError::Poisoned(poisoned)) => Some(poisoned.into_inner()),
                Err(TryLockError::WouldBlock) => None,
            })
        })?
        .ok_or(OrchestrationError::AskNotTyped)?;
        poll_until(deadline, || {
            if !self.host.running(session) {
                return Err(OrchestrationError::Host(
                    "the worker has ended, so there is no one to answer".to_owned(),
                ));
            }
            Ok(self.host.ready_for_input(session).then_some(()))
        })?
        .ok_or(OrchestrationError::AskNotTyped)?;
        let before: HashSet<String> = self
            .team
            .messages_of(&member.id)?
            .into_iter()
            .map(|message| message.id)
            .collect();
        on_typed()?;
        self.host
            .send_pasted(session, text)
            .map_err(OrchestrationError::Host)?;
        let typed_at = Instant::now();
        let mut busy = false;
        let mut report: Option<TeamMessage> = None;
        let over = poll_until(deadline, || {
            let status = self.host.status(session);
            busy |= matches!(status, Some(AgentStatus::Working | AgentStatus::NeedsYou));
            report = self.report_after(&member.id, &before)?;
            if report.is_some() || !self.host.running(session) {
                return Ok(Some(()));
            }
            let idle = matches!(status, Some(AgentStatus::Idle | AgentStatus::Done));
            Ok((idle && (busy || typed_at.elapsed() >= IDLE_AFTER_ASK)).then_some(()))
        })?;
        if over.is_none() {
            return Err(OrchestrationError::AskNoAnswer);
        }
        Ok(report)
    }

    /// Holds the ask `from` makes of `to` for as long as it lasts. It is refused at once when the two are the same
    /// member, and when `to` already waits for `from`, directly or through others: neither could answer the other.
    fn start_ask(&self, from: &str, to: &str) -> Result<AskEdge<'_>, OrchestrationError> {
        if from == to {
            return Err(OrchestrationError::AskSelf);
        }
        let mut waits = self.waits.lock().unwrap_or_else(PoisonError::into_inner);
        if waits_through(&waits, to, from) {
            return Err(OrchestrationError::AskCycle);
        }
        let edge = (from.to_owned(), to.to_owned());
        waits.push(edge.clone());
        Ok(AskEdge {
            waits: &self.waits,
            edge,
        })
    }

    /// Keeps a question or an answer between two members of one team. A line the caller already has is kept read.
    fn record(
        &self,
        kind: MessageKind,
        from: &str,
        to: &str,
        body: &str,
        read: bool,
    ) -> Result<(), OrchestrationError> {
        let exchange = TeamExchange {
            id: new_id(),
            kind,
            from_member_id: from.to_owned(),
            to_member_id: to.to_owned(),
            body: body.to_owned(),
            created_at: now(),
        };
        self.team.add_exchange(&exchange)?;
        if read {
            self.team.mark_message_read(&exchange.id)?;
        }
        Ok(())
    }

    /// The gate that holds `member_id`'s question until its answer. Made on first use.
    fn ask_gate(&self, member_id: &str) -> Arc<Mutex<()>> {
        self.asks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(member_id.to_owned())
            .or_default()
            .clone()
    }

    /// The first report of the member that was not there before the question, if one came.
    fn report_after(
        &self,
        member_id: &str,
        before: &HashSet<String>,
    ) -> Result<Option<TeamMessage>, OrchestrationError> {
        Ok(self
            .team
            .messages_of(member_id)?
            .into_iter()
            .rev()
            .find(|message| {
                matches!(message.kind, MessageKind::Done | MessageKind::Failed)
                    && !before.contains(&message.id)
            }))
    }

    /// What a worker's terminal shows. A closed terminal has no screen left, whether or not the app has noticed yet.
    fn screen_of(&self, session: &str) -> Result<String, OrchestrationError> {
        match self.host.output(session) {
            Ok(text) => Ok(text),
            Err(_) if !self.host.running(session) => Ok(String::new()),
            Err(error) => Err(OrchestrationError::Host(error)),
        }
    }

    /// The member of the team at `worktree` that `member_ref` names, by id or by role. A role shared by several
    /// members must be told apart by id.
    fn teammate(&self, worktree: &str, member_ref: &str) -> Result<TeamMember, OrchestrationError> {
        let members: Vec<TeamMember> = self
            .team
            .members()?
            .into_iter()
            .filter(|member| member.worktree == worktree)
            .collect();
        if let Some(member) = members.iter().find(|member| member.id == member_ref) {
            return Ok(member.clone());
        }
        let wanted = member_ref.trim();
        let named: Vec<&TeamMember> = members
            .iter()
            .filter(|member| member.role.eq_ignore_ascii_case(wanted))
            .collect();
        match named.as_slice() {
            [only] => Ok((*only).clone()),
            [] => Err(OrchestrationError::NotInYourTeam(member_ref.to_owned())),
            several => Err(OrchestrationError::AmbiguousMember(
                wanted.to_owned(),
                several
                    .iter()
                    .map(|member| member.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
            )),
        }
    }

    /// Ends a worker. Its task is cancelled if it was still running.
    pub fn stop_worker(&self, caller: &str, member_id: &str) -> Result<(), OrchestrationError> {
        let parents = self.orchestrators(caller)?;
        let member = self.child_of(&parents, member_id)?;
        if member.dispatch_state == Some(DispatchState::Running) {
            if let Some(session) = &member.session_id {
                self.host.stop(session);
            }
            self.team
                .set_dispatch_state(&member.id, DispatchState::Stopped)?;
        }
        if member.task_state == Some(TaskState::Running) {
            self.team.set_task_state(&member.id, TaskState::Cancelled)?;
        }
        self.host.changed();
        Ok(())
    }

    /// Notices the workers of these orchestrators that ended without a report, and for an agent that cannot report,
    /// the end of its turn. Each notice is written once.
    fn notice_workers(&self, parent_ids: &[String]) -> Result<(), OrchestrationError> {
        let members = self.team.members()?;
        for worker in members.iter().filter(|member| {
            member
                .parent_id
                .as_ref()
                .is_some_and(|parent| parent_ids.contains(parent))
                && is_active(member)
        }) {
            let Some(session) = worker.session_id.as_deref() else {
                continue;
            };
            let waiting = self.host.running(session)
                && self.host.status(session) == Some(AgentStatus::NeedsYou);
            let was_waiting = self
                .waiting
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(worker.id.clone(), waiting)
                .unwrap_or(false);
            if waiting && !was_waiting {
                self.note(
                    worker,
                    MessageKind::NeedsYou,
                    "The worker is waiting for an answer. Read its output to see the question; the user gives the answer in the worker's terminal.",
                )?;
            }
            if !self.host.running(session) {
                self.team
                    .set_dispatch_state(&worker.id, DispatchState::Stopped)?;
                self.team.set_task_state(&worker.id, TaskState::Failed)?;
                self.note(
                    worker,
                    MessageKind::NoReport,
                    "The worker ended without a report. Read its output to see how far it got.",
                )?;
            } else if self.host.status(session) == Some(AgentStatus::Done)
                && !self
                    .team
                    .messages_of(&worker.id)?
                    .iter()
                    .any(|message| message.kind == MessageKind::NoReport)
            {
                // The turn ended while the task still runs and no report came: the orchestrator hears it once. The
                // worker keeps running, and it may be waiting for an answer.
                self.note(
                    worker,
                    MessageKind::NoReport,
                    "The worker finished its turn without a report. Read its output to see what it did, or whether it waits for an answer.",
                )?;
            }
        }
        Ok(())
    }

    /// Closes the workers whose reports the orchestrator has just heard, when Settings say so. The report stays with
    /// the orchestrator; what the worker printed goes with its terminal.
    fn release_reported(&self, messages: &[TeamMessage]) -> Result<(), OrchestrationError> {
        if !self.host.settings().release_finished_workers {
            return Ok(());
        }
        let members = self.team.members()?;
        for message in messages {
            if !matches!(message.kind, MessageKind::Done | MessageKind::Failed) {
                continue;
            }
            let Some(worker) = members.iter().find(|member| member.id == message.member_id) else {
                continue;
            };
            if worker.dispatch_state == Some(DispatchState::Running) {
                if let Some(session) = &worker.session_id {
                    self.host.stop(session);
                }
                self.team
                    .set_dispatch_state(&worker.id, DispatchState::Stopped)?;
                self.host.changed();
            }
        }
        Ok(())
    }

    fn worker_states(&self, parent_ids: &[String]) -> Result<Vec<WorkerState>, OrchestrationError> {
        Ok(self
            .team
            .members()?
            .iter()
            .filter(|member| {
                member.dispatch_state.is_some()
                    && member
                        .parent_id
                        .as_ref()
                        .is_some_and(|parent| parent_ids.contains(parent))
            })
            .map(|member| WorkerState {
                member_id: member.id.clone(),
                role: member.role.clone(),
                harness: member.harness,
                task_state: member.task_state.unwrap_or(TaskState::Pending),
                running: self.is_running(member),
            })
            .collect())
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

/// A bot's member ids, one per worktree it is in: the parents its workers answer to.
fn bot_member_ids<'a>(members: &'a [TeamMember], bot: &str) -> Vec<&'a str> {
    members
        .iter()
        .filter(|member| {
            member.kind == TeamMemberKind::Bot && member.bot_id.as_deref() == Some(bot)
        })
        .map(|member| member.id.as_str())
        .collect()
}

/// A member and every member under it at any depth.
fn subtree<'a>(members: &'a [TeamMember], root: &str) -> Vec<&'a TeamMember> {
    let mut found: Vec<&TeamMember> = members.iter().filter(|member| member.id == root).collect();
    let mut next = 0;
    while let Some(parent) = found.get(next).map(|member| member.id.clone()) {
        next += 1;
        for member in members {
            if member.parent_id.as_deref() == Some(parent.as_str())
                && !found.iter().any(|known| known.id == member.id)
            {
                found.push(member);
            }
        }
    }
    found
}

/// A worker is active while it runs and its task has no report yet. Its process may have ended: that is what the
/// checks notice.
fn is_active(member: &TeamMember) -> bool {
    member.dispatch_state == Some(DispatchState::Running)
        && member.task_state == Some(TaskState::Running)
}

/// Whether an agent reports through the lynx tools. Oh My Pi cannot, so its end of turn is what the app sees.
fn reports_through_tools(harness: HarnessId) -> bool {
    harness != HarnessId::Omp
}

/// The name the agents tab shows for a worker's agent status.
pub fn agent_status_name(status: AgentStatus) -> String {
    match status {
        AgentStatus::Working => "working",
        AgentStatus::NeedsYou => "needs_you",
        AgentStatus::Done => "done",
        AgentStatus::Idle => "idle",
    }
    .to_owned()
}

/// The task a worker is told. A replacement is told that another agent began it.
fn worker_task(member: &TeamMember, continuing: bool) -> String {
    if continuing {
        format!(
            "Another agent began this task in this worktree and has been replaced by you. Run git status and git diff to see what was done, then carry on.\n\n{}",
            member.task
        )
    } else {
        member.task.clone()
    }
}

/// What a worker reads: its role's prompt when the role is a preset, its task, and for an agent that reports through
/// the tools, how to report and where the team's notes are.
fn worker_prompt(harness: HarnessId, role: Option<&str>, task: &str) -> String {
    let role = role.map_or_else(String::new, |prompt| format!("Your role:\n{prompt}\n\n"));
    if reports_through_tools(harness) {
        format!(
            "{role}{task}\n\nThe team shares notes: lynx_note_read lists them and reads one, lynx_note_write and lynx_note_edit change them.\n\nTo ask a member of your team something, call lynx_member_ask with its id or role and your question. The call waits for the answer, so ask only what you need.\n\nWhen you finish, or if you cannot, report to your orchestrator with lynx_report (outcome \"done\" or \"failed\" and a short summary). lynx_report is a Lynx tool: call it from your shell through the lynx skill, as the skill describes."
        )
    } else {
        format!("{role}{task}")
    }
}

/// Whether a member that waits for these ids must wait: one of them is not a done member of the team. A name of a
/// member of the same batch is not in the team yet, so it waits.
fn waits_for_tasks(members: &[TeamMember], depends_on: &[String]) -> bool {
    depends_on.iter().any(|id| {
        members
            .iter()
            .find(|other| &other.id == id)
            .is_none_or(|other| other.task_state != Some(TaskState::Done))
    })
}

/// An ask in progress: the edge that says its asker waits for the member it asked. It leaves the wait map when the
/// ask ends, however it ends.
struct AskEdge<'a> {
    waits: &'a Mutex<Vec<(String, String)>>,
    edge: (String, String),
}

impl Drop for AskEdge<'_> {
    fn drop(&mut self) {
        let mut waits = self.waits.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(at) = waits.iter().position(|edge| edge == &self.edge) {
            waits.remove(at);
        }
    }
}

/// Whether `from` waits for `to`, directly or through members that wait for others, along the asks in `waits`.
fn waits_through(waits: &[(String, String)], from: &str, to: &str) -> bool {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut pending = vec![from];
    while let Some(current) = pending.pop() {
        if current == to {
            return true;
        }
        if !seen.insert(current) {
            continue;
        }
        pending.extend(
            waits
                .iter()
                .filter(|(asker, _)| asker.as_str() == current)
                .map(|(_, asked)| asked.as_str()),
        );
    }
    false
}

/// The answer kept in the team: the report when there is one, else the end of the screen, else a line that says the
/// worker showed nothing, so every answer keeps some text.
fn answer_body(report: Option<&str>, screen: &str) -> String {
    let text = match report {
        Some(report) => report.trim().to_owned(),
        None => last_chars(screen.trim(), MAX_EXCHANGE_CHARS)
            .trim()
            .to_owned(),
    };
    if text.is_empty() {
        "The worker showed no answer.".to_owned()
    } else {
        text
    }
}

/// The last `count` characters of `text`, or all of it when it is shorter.
fn last_chars(text: &str, count: usize) -> &str {
    let start = text
        .char_indices()
        .rev()
        .nth(count.saturating_sub(1))
        .map_or(0, |(index, _)| index);
    &text[start..]
}

/// Looks again every `ASK_POLL` until `look` finds something, or `deadline` passes: then None.
fn poll_until<T>(
    deadline: Instant,
    mut look: impl FnMut() -> Result<Option<T>, OrchestrationError>,
) -> Result<Option<T>, OrchestrationError> {
    loop {
        if let Some(found) = look()? {
            return Ok(Some(found));
        }
        if Instant::now() >= deadline {
            return Ok(None);
        }
        std::thread::sleep(ASK_POLL);
    }
}

/// A question is text the worker reads as its input: line breaks stay, other control characters are refused.
fn clean_question(text: &str) -> Result<String, OrchestrationError> {
    let text = text.trim().replace("\r\n", "\n");
    let control = |c: char| c.is_control() && c != '\n' && c != '\t';
    if text.is_empty() || text.chars().count() > MAX_ASK || text.chars().any(control) {
        return Err(OrchestrationError::InvalidText(format!(
            "a question is 1 to {MAX_ASK} characters, with no control characters but line breaks"
        )));
    }
    Ok(text)
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

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
        })
}
