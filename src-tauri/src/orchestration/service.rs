//! The coordination rules: who may add, start, read, answer or stop a worker, when a task may start, and what an
//! orchestrator hears back. The workers are members of the team, so the team holds what they do. Terminals and
//! worktrees come through `WorkerHost`, so the rules are tested without them.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;

use crate::agents::activity::AgentStatus;
use crate::agents::harness::{HarnessId, harness_name};
use crate::agents::launch_line::unattended_settings;
use crate::bots::bot_bridge::bot_of_session;
use crate::config::app_config::OrchestrationConfig;
use crate::persistence::DatabaseError;
use crate::team::team_error::TeamError;
use crate::team::team_model::{
    DispatchState, MessageKind, NewTeamMember, Outcome, TaskState, TeamMember, TeamMemberKind,
    TeamMessage, TeamSnapshot, worker_card_state,
};
use crate::team::team_service::{TeamService, new_id};

const MAX_REPORT_CHARS: usize = 4_000;
const MAX_MODEL_CHARS: usize = 100;
/// The longest answer a coordinator may type into a worker's terminal, in characters.
const MAX_REPLY: usize = 2000;
/// How often a check looks again while it waits for something to arrive.
const CHECK_POLL: Duration = Duration::from_millis(500);

/// Where a worker works: in a worktree of its own, or in the team's worktree, where the orchestrator works.
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

/// What the rules need from the rest of the app.
pub trait WorkerHost: Send + Sync + 'static {
    /// The orchestration settings as the user has them now.
    fn settings(&self) -> OrchestrationConfig;
    /// A new worktree called `name` in the repository `from` belongs to.
    fn new_worktree(&self, from: &str, name: &str) -> Result<String, String>;
    /// Writes `text` where an agent can read it, and returns the path.
    fn prompt_file(&self, text: &str) -> Result<String, String>;
    /// The line that starts an agent, with the launch mode and arguments Settings hold for it.
    /// `unattended` says nobody watches its terminal, so it must not stop to ask for permissions.
    fn launch_line(&self, harness: HarnessId, unattended: bool) -> String;
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

pub struct Orchestration {
    team: Arc<TeamService>,
    host: Arc<dyn WorkerHost>,
    /// Which workers were waiting for an answer at the last look, so each wait is said once.
    waiting: Mutex<HashMap<String, bool>>,
}

impl Orchestration {
    pub fn new(team: Arc<TeamService>, host: Arc<dyn WorkerHost>) -> Self {
        Self {
            team,
            host,
            waiting: Mutex::new(HashMap::new()),
        }
    }

    /// Adds a member under one of the caller's orchestrators: a task and a "done when" line, and what it waits for.
    /// The member has no agent until a worker starts for it.
    pub fn add_member(
        &self,
        caller: &str,
        parent: Option<&str>,
        role: &str,
        task: &str,
        acceptance: &str,
        depends_on: Vec<String>,
    ) -> Result<TeamMember, OrchestrationError> {
        let parents = self.orchestrators(caller)?;
        let parent = match (parent, parents.as_slice()) {
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
        if task.trim().is_empty() {
            return Err(OrchestrationError::InvalidText(
                "a member needs a task: say what it must do".to_owned(),
            ));
        }
        let member = self.team.add_member(NewTeamMember::Agent {
            role: role.to_owned(),
            harness: None,
            parent_id: Some(parent.id.clone()),
            orchestrator: false,
            task: task.to_owned(),
            acceptance: acceptance.to_owned(),
            depends_on,
            worktree: parent.worktree.clone(),
            session_id: None,
        })?;
        self.host.changed();
        Ok(member)
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
        member_id: &str,
        harness: HarnessId,
        worktree: WorkerWorktree,
        model: Option<&str>,
    ) -> Result<WorkerStart, OrchestrationError> {
        let parents = self.orchestrators(caller)?;
        let member = self.child_of(&parents, member_id)?;
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
        if !waiting.is_empty() {
            return Err(OrchestrationError::WaitingFor(waiting.join(", ")));
        }
        let project = member.worktree.clone();
        self.host
            .allow_start(caller, &project)
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
        let execution = match worktree {
            WorkerWorktree::Current => project.clone(),
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
        let prompt = self
            .host
            .prompt_file(&worker_prompt(harness, &member.task))
            .map_err(OrchestrationError::Host)?;
        // A bot does not watch its workers' terminals, so they never stop to ask for permissions.
        let unattended = bot_of_session(caller).is_some();
        let line = self.host.launch_line(harness, unattended);
        // Nobody answers an unattended worker's first-run prompts, so its settings turn them off.
        let line = match unattended_settings(harness) {
            Some(settings) if unattended => format!(
                "{line} --settings {}",
                quote_for_shell(settings, cfg!(windows))
            ),
            _ => line,
        };
        let instruction = read_instruction(&prompt);
        let command = agent_command(harness, &line, &instruction, model);
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
            member_id: member.id,
            harness,
            session_id: session,
            worktree_path: execution,
        })
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

    /// The member `member_id`, when it answers to one of the orchestrators. Nothing else is the caller's to manage.
    fn child_of(
        &self,
        parents: &[TeamMember],
        member_id: &str,
    ) -> Result<TeamMember, OrchestrationError> {
        self.team
            .members()?
            .into_iter()
            .find(|member| {
                member.id == member_id
                    && member
                        .parent_id
                        .as_ref()
                        .is_some_and(|parent| parents.iter().any(|mine| &mine.id == parent))
            })
            .ok_or_else(|| OrchestrationError::UnknownMember(member_id.to_owned()))
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
                    MessageKind::NoReport | MessageKind::NeedsYou => None,
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
        let output = match self.host.output(&session) {
            Ok(text) => text,
            // A closed terminal has no screen left, whether or not the app has noticed yet.
            Err(_) if !self.host.running(&session) => String::new(),
            Err(error) => return Err(OrchestrationError::Host(error)),
        };
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
fn agent_status_name(status: AgentStatus) -> String {
    match status {
        AgentStatus::Working => "working",
        AgentStatus::NeedsYou => "needs_you",
        AgentStatus::Done => "done",
        AgentStatus::Idle => "idle",
    }
    .to_owned()
}

/// What a worker reads: its task, and for an agent that reports through the tools, how to report.
fn worker_prompt(harness: HarnessId, task: &str) -> String {
    if reports_through_tools(harness) {
        format!(
            "{task}\n\nWhen you finish, or if you cannot, report to your orchestrator with lynx_report (outcome \"done\" or \"failed\" and a short summary). lynx_report is a Lynx tool: call it from your shell through the lynx skill, as the skill describes."
        )
    } else {
        task.to_owned()
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

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
        })
}
