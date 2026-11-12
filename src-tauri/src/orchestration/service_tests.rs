//! The coordination rules, tested with a host in memory: no terminal, no worktree, no settings file.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::agents::activity::AgentStatus;
use crate::agents::harness::HarnessId;
use crate::config::app_config::{AgentEffort, OrchestrationConfig};
use crate::persistence::AppDatabase;
use crate::team::team_error::TeamError;
use crate::team::team_model::{
    DispatchState, MessageKind, NewTeamMember, Outcome, TaskState, TeamMember, TeamMemberKind,
};
use crate::team::team_service::TeamService;

use super::agent_wake::free_to_type;
use super::service::{
    Assignment, LaunchSetup, NewMember, Orchestration, OrchestrationError, WorkerHost,
    WorkerWorktree,
};

/// The terminals and worktrees of a test: what it started, stopped and was asked to type.
#[derive(Default)]
pub struct FakeHost {
    /// Whether orchestration asks for a worker's reports to close the worker, as the setting says.
    pub release: bool,
    /// Whether orchestration is off in Lynx's settings.
    pub off: bool,
    running: Mutex<Vec<String>>,
    statuses: Mutex<HashMap<String, AgentStatus>>,
    drafts: Mutex<Vec<String>>,
    started: Mutex<Vec<(String, String)>>,
    stopped: Mutex<Vec<String>>,
    notified: Mutex<Vec<String>>,
    sent: Mutex<Vec<(String, String)>>,
    prompts: Mutex<Vec<String>>,
    landed: Mutex<Vec<(String, String)>>,
    efforts: Mutex<Vec<Option<AgentEffort>>>,
}

impl FakeHost {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn releasing() -> Self {
        Self {
            release: true,
            ..Self::default()
        }
    }

    pub fn disabled() -> Self {
        Self {
            off: true,
            ..Self::default()
        }
    }

    /// Whether the person has text in the session's input, which holds back a question.
    pub fn set_draft(&self, session: &str, draft: bool) {
        let mut drafts = self
            .drafts
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        drafts.retain(|other| other != session);
        if draft {
            drafts.push(session.to_owned());
        }
    }

    /// The effort each launch line was built with, in order.
    pub fn efforts(&self) -> Vec<Option<AgentEffort>> {
        self.efforts
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    /// The worker's process ended on its own, as an agent does when it exits.
    pub fn end(&self, session: &str) {
        self.running
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .retain(|running| running != session);
    }

    pub fn set_status(&self, session: &str, status: AgentStatus) {
        self.statuses
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(session.to_owned(), status);
    }

    /// The (worktree, command) of every terminal started, in order.
    pub fn started(&self) -> Vec<(String, String)> {
        self.started
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    pub fn stopped(&self) -> Vec<String> {
        self.stopped
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    pub fn notified(&self) -> Vec<String> {
        self.notified
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    /// The text of every prompt file written, in order.
    pub fn prompts(&self) -> Vec<String> {
        self.prompts
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    pub fn landed(&self) -> Vec<(String, String)> {
        self.landed
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    pub fn sent(&self) -> Vec<(String, String)> {
        self.sent
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }
}

impl WorkerHost for FakeHost {
    fn settings(&self) -> OrchestrationConfig {
        OrchestrationConfig {
            enabled: !self.off,
            release_finished_workers: self.release,
        }
    }

    fn new_worktree(&self, from: &str, name: &str) -> Result<String, String> {
        Ok(format!("{from}/{name}"))
    }

    fn prompt_file(&self, text: &str) -> Result<String, String> {
        self.prompts
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(text.to_owned());
        Ok("/prompts/task.md".to_owned())
    }

    fn launch_line(
        &self,
        harness: HarnessId,
        _unattended: bool,
        effort: Option<AgentEffort>,
    ) -> String {
        self.efforts
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(effort);
        format!("agent-{}", crate::agents::harness::harness_name(harness))
    }

    fn ready_for_input(&self, session: &str) -> bool {
        let drafted = self
            .drafts
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
            .any(|other| other == session);
        free_to_type(self.status(session), drafted)
    }

    fn start(&self, worktree: &str, command: &str) -> Result<String, String> {
        let mut started = self
            .started
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let session = format!("session-{}", started.len() + 1);
        started.push((worktree.to_owned(), command.to_owned()));
        self.running
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(session.clone());
        Ok(session)
    }

    fn running(&self, session: &str) -> bool {
        self.running
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
            .any(|running| running == session)
    }

    fn status(&self, session: &str) -> Option<AgentStatus> {
        self.statuses
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get(session)
            .copied()
    }

    fn output(&self, session: &str) -> Result<String, String> {
        Ok(format!("the screen of {session}"))
    }

    fn stop(&self, session: &str) {
        self.end(session);
        self.stopped
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(session.to_owned());
    }

    fn notify(&self, session: &str) {
        self.notified
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(session.to_owned());
    }

    fn land(&self, from: &str, into: &str) -> Result<String, String> {
        self.landed
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push((from.to_owned(), into.to_owned()));
        Ok("merged".to_owned())
    }

    fn send(&self, session: &str, text: &str) -> Result<(), String> {
        self.sent
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push((session.to_owned(), text.to_owned()));
        Ok(())
    }

    fn send_pasted(&self, session: &str, text: &str) -> Result<(), String> {
        self.send(session, text)
    }
}

/// A team with one orchestrator agent, `lead`, in `/repo/app`, and an orchestration over it.
pub struct Fixture {
    _dir: tempfile::TempDir,
    pub team: Arc<TeamService>,
    pub host: Arc<FakeHost>,
    pub orchestration: Orchestration,
    pub lead: TeamMember,
}

pub const LEAD_SESSION: &str = "lead-session";
pub const WORKTREE: &str = if cfg!(windows) {
    r"C:\repo\app"
} else {
    "/repo/app"
};
pub const OTHER_WORKTREE: &str = if cfg!(windows) {
    r"C:\repo\docs"
} else {
    "/repo/docs"
};

pub fn fixture_with(host: FakeHost) -> Fixture {
    let dir = tempfile::tempdir().expect("temp dir");
    let database = Arc::new(AppDatabase::open(&dir.path().join("app.db")).expect("database"));
    let team = Arc::new(TeamService::new(Arc::clone(&database), Box::new(|_| true)));
    let host = Arc::new(host);
    let orchestration =
        Orchestration::new(Arc::clone(&team), Arc::clone(&host) as Arc<dyn WorkerHost>);
    let lead = team
        .add_member(NewTeamMember::Agent {
            role: "Lead".to_owned(),
            harness: Some(HarnessId::Claude),
            parent_id: None,
            orchestrator: true,
            task: String::new(),
            acceptance: String::new(),
            depends_on: Vec::new(),
            worktree: WORKTREE.to_owned(),
            session_id: Some(LEAD_SESSION.to_owned()),
            key: None,
            start_when_released: false,
        })
        .expect("the lead joins the team");
    Fixture {
        _dir: dir,
        team,
        host,
        orchestration,
        lead,
    }
}

pub fn fixture() -> Fixture {
    fixture_with(FakeHost::new())
}

impl Fixture {
    /// Adds a member under the lead, with a task and what it waits for.
    pub fn add(&self, role: &str, task: &str, depends_on: Vec<String>) -> TeamMember {
        self.orchestration
            .add_member(LEAD_SESSION, None, role, task, "", depends_on)
            .expect("the lead adds a member")
    }

    pub fn member(&self, id: &str) -> TeamMember {
        self.team
            .members()
            .expect("members")
            .into_iter()
            .find(|member| member.id == id)
            .expect("the member is in the team")
    }
}

#[test]
fn a_member_added_by_the_lead_is_a_pending_task_with_no_agent_yet() {
    let fixture = fixture();

    let writer = fixture.add("Writer", "Escreva o guia", Vec::new());

    assert_eq!(writer.parent_id.as_deref(), Some(fixture.lead.id.as_str()));
    assert_eq!(writer.task_state, Some(TaskState::Pending));
    assert_eq!(
        writer.harness, None,
        "an agent is chosen when the worker starts"
    );
    assert_eq!(
        writer.worktree, WORKTREE,
        "the member joins the lead's team"
    );
}

#[test]
fn a_member_is_only_added_by_an_orchestrator_with_a_task() {
    let fixture = fixture();
    assert!(matches!(
        fixture
            .orchestration
            .add_member(LEAD_SESSION, None, "Writer", "  ", "", Vec::new()),
        Err(OrchestrationError::InvalidText(_))
    ));

    assert_eq!(
        fixture
            .orchestration
            .add_member("stranger", None, "Writer", "Escreva", "", Vec::new()),
        Err(OrchestrationError::NotInTeam)
    );

    let plain = fixture
        .team
        .add_member(NewTeamMember::Agent {
            role: "Plain".to_owned(),
            harness: Some(HarnessId::Codex),
            parent_id: None,
            orchestrator: false,
            task: String::new(),
            acceptance: String::new(),
            depends_on: Vec::new(),
            worktree: WORKTREE.to_owned(),
            session_id: Some("plain-session".to_owned()),
            key: None,
            start_when_released: false,
        })
        .expect("a plain member");
    assert!(!plain.orchestrator);
    assert_eq!(
        fixture.orchestration.add_member(
            "plain-session",
            None,
            "Writer",
            "Escreva",
            "",
            Vec::new()
        ),
        Err(OrchestrationError::NotAnOrchestrator)
    );
}

#[test]
fn an_orchestrator_only_manages_its_own_members() {
    let fixture = fixture();
    let writer = fixture.add("Writer", "Escreva", Vec::new());
    let other = fixture
        .team
        .add_member(NewTeamMember::Agent {
            role: "Other".to_owned(),
            harness: None,
            parent_id: None,
            orchestrator: true,
            task: String::new(),
            acceptance: String::new(),
            depends_on: Vec::new(),
            worktree: WORKTREE.to_owned(),
            session_id: Some("other-session".to_owned()),
            key: None,
            start_when_released: false,
        })
        .expect("another orchestrator");
    let theirs = fixture
        .orchestration
        .add_member("other-session", None, "Theirs", "Revise", "", Vec::new())
        .expect("the other orchestrator adds one");

    assert!(matches!(
        fixture.orchestration.start_worker(
            LEAD_SESSION,
            &theirs.id,
            HarnessId::Claude,
            WorkerWorktree::Current,
            LaunchSetup::default()
        ),
        Err(OrchestrationError::UnknownMember(_))
    ));
    assert!(matches!(
        fixture
            .orchestration
            .dismiss_member(LEAD_SESSION, &theirs.id),
        Err(OrchestrationError::UnknownMember(_))
    ));
    fixture
        .orchestration
        .dismiss_member(LEAD_SESSION, &writer.id)
        .expect("its own member goes");
    assert_ne!(other.id, writer.id);
}

#[test]
fn a_worker_starts_for_its_member_and_its_report_reaches_the_lead() {
    let fixture = fixture();
    let writer = fixture.add("Writer", "Escreva o guia", Vec::new());

    let started = fixture
        .orchestration
        .start_worker(
            LEAD_SESSION,
            &writer.id,
            HarnessId::Claude,
            WorkerWorktree::Current,
            LaunchSetup::default(),
        )
        .expect("the worker starts");

    assert_eq!(started.harness, HarnessId::Claude);
    let member = fixture.member(&writer.id);
    assert_eq!(member.task_state, Some(TaskState::Running));
    assert_eq!(member.dispatch_state, Some(DispatchState::Running));
    assert_eq!(
        member.session_id.as_deref(),
        Some(started.session_id.as_str())
    );
    assert_eq!(
        member.worktree, WORKTREE,
        "the team's worktree is the member's own"
    );
    assert_eq!(member.execution_worktree.as_deref(), Some(WORKTREE));
    assert!(
        fixture.host.started()[0]
            .1
            .contains("Read /prompts/task.md"),
        "the worker reads its task from the file"
    );

    fixture
        .orchestration
        .report(&started.session_id, Outcome::Done, "Guia escrito")
        .expect("the worker reports");

    let member = fixture.member(&writer.id);
    assert_eq!(member.task_state, Some(TaskState::Done));
    let check = fixture
        .orchestration
        .check(LEAD_SESSION, Duration::ZERO)
        .expect("the lead checks");
    assert_eq!(check.messages.len(), 1);
    assert_eq!(check.messages[0].kind, MessageKind::Done);
    assert_eq!(check.messages[0].member_id, writer.id);
    assert_eq!(check.messages[0].body, "Guia escrito");
    assert!(
        fixture
            .orchestration
            .check(LEAD_SESSION, Duration::ZERO)
            .expect("the lead checks again")
            .messages
            .is_empty(),
        "a report reaches the lead once"
    );
    assert_eq!(fixture.host.notified(), vec![LEAD_SESSION.to_owned()]);
}

#[test]
fn a_task_waits_for_the_tasks_it_depends_on() {
    let fixture = fixture();
    let research = fixture.add("Researcher", "Pesquise", Vec::new());
    let tests = fixture.add("Tester", "Teste", vec![research.id.clone()]);

    assert!(matches!(
        fixture
            .orchestration
            .start_worker(LEAD_SESSION, &tests.id, HarnessId::Claude, WorkerWorktree::Current, LaunchSetup::default()),
        Err(OrchestrationError::WaitingFor(names)) if names == "Researcher"
    ));

    let started = fixture
        .orchestration
        .start_worker(
            LEAD_SESSION,
            &research.id,
            HarnessId::Claude,
            WorkerWorktree::Current,
            LaunchSetup::default(),
        )
        .expect("the first starts");
    fixture
        .orchestration
        .report(&started.session_id, Outcome::Done, "Pronto")
        .expect("it reports");

    fixture
        .orchestration
        .start_worker(
            LEAD_SESSION,
            &tests.id,
            HarnessId::Claude,
            WorkerWorktree::Current,
            LaunchSetup::default(),
        )
        .expect("the second starts once the first is done");
}

#[test]
fn a_worker_in_a_new_worktree_keeps_its_place_under_the_lead() {
    let fixture = fixture();
    let writer = fixture.add("Writer", "Escreva", Vec::new());

    let started = fixture
        .orchestration
        .start_worker(
            LEAD_SESSION,
            &writer.id,
            HarnessId::Codex,
            WorkerWorktree::New,
            LaunchSetup::default(),
        )
        .expect("the worker starts in a worktree of its own");

    assert_eq!(
        started.worktree_path,
        format!("{WORKTREE}/lynx-task-{}", &writer.id[..8])
    );
    let member = fixture.member(&writer.id);
    assert_eq!(
        member.worktree, WORKTREE,
        "the tree stays in the lead's worktree"
    );
    assert_eq!(member.parent_id.as_deref(), Some(fixture.lead.id.as_str()));
    assert_eq!(
        member.execution_worktree.as_deref(),
        Some(started.worktree_path.as_str())
    );
}

#[test]
fn a_worker_cannot_coordinate_or_report_for_another_member() {
    let fixture = fixture();
    let writer = fixture.add("Writer", "Escreva", Vec::new());
    let started = fixture
        .orchestration
        .start_worker(
            LEAD_SESSION,
            &writer.id,
            HarnessId::Claude,
            WorkerWorktree::Current,
            LaunchSetup::default(),
        )
        .expect("the worker starts");

    assert_eq!(
        fixture
            .orchestration
            .add_member(&started.session_id, None, "Sub", "Faça", "", Vec::new()),
        Err(OrchestrationError::WorkerCannotCoordinate)
    );
    assert_eq!(
        fixture
            .orchestration
            .report("unknown-session", Outcome::Done, "x"),
        Err(OrchestrationError::NotAWorker)
    );
}

#[test]
fn a_member_with_a_running_worker_cannot_leave_the_team() {
    let fixture = fixture();
    let writer = fixture.add("Writer", "Escreva", Vec::new());
    let started = fixture
        .orchestration
        .start_worker(
            LEAD_SESSION,
            &writer.id,
            HarnessId::Claude,
            WorkerWorktree::Current,
            LaunchSetup::default(),
        )
        .expect("the worker starts");

    assert!(matches!(
        fixture
            .orchestration
            .dismiss_member(LEAD_SESSION, &writer.id),
        Err(OrchestrationError::WorkerRunning(_))
    ));

    fixture
        .orchestration
        .stop_worker(LEAD_SESSION, &writer.id)
        .expect("the worker stops");
    fixture
        .orchestration
        .dismiss_member(LEAD_SESSION, &writer.id)
        .expect("a stopped worker's member can leave");
    assert!(
        fixture
            .team
            .members()
            .expect("members")
            .iter()
            .all(|m| m.id != writer.id)
    );
    assert_eq!(fixture.host.stopped(), vec![started.session_id]);
}

#[test]
fn a_stopped_worker_cancels_its_task_and_cannot_report() {
    let fixture = fixture();
    let writer = fixture.add("Writer", "Escreva", Vec::new());
    let started = fixture
        .orchestration
        .start_worker(
            LEAD_SESSION,
            &writer.id,
            HarnessId::Claude,
            WorkerWorktree::Current,
            LaunchSetup::default(),
        )
        .expect("the worker starts");

    fixture
        .orchestration
        .stop_worker(LEAD_SESSION, &writer.id)
        .expect("the lead stops it");

    assert_eq!(
        fixture.member(&writer.id).task_state,
        Some(TaskState::Cancelled)
    );
    assert_eq!(
        fixture
            .orchestration
            .report(&started.session_id, Outcome::Done, "tarde"),
        Err(OrchestrationError::WorkerStopped)
    );
}

#[test]
fn a_worker_that_ends_without_a_report_fails_its_task_and_says_so() {
    let fixture = fixture();
    let writer = fixture.add("Writer", "Escreva", Vec::new());
    let started = fixture
        .orchestration
        .start_worker(
            LEAD_SESSION,
            &writer.id,
            HarnessId::Claude,
            WorkerWorktree::Current,
            LaunchSetup::default(),
        )
        .expect("the worker starts");
    fixture.host.end(&started.session_id);

    let check = fixture
        .orchestration
        .check(LEAD_SESSION, Duration::ZERO)
        .expect("the lead checks");

    assert_eq!(check.messages.len(), 1);
    assert_eq!(check.messages[0].kind, MessageKind::NoReport);
    assert_eq!(
        fixture.member(&writer.id).task_state,
        Some(TaskState::Failed)
    );
    assert_eq!(
        fixture.member(&writer.id).dispatch_state,
        Some(DispatchState::Stopped)
    );
}

#[test]
fn a_worker_that_waits_for_an_answer_is_said_once_and_can_be_answered() {
    let fixture = fixture();
    let writer = fixture.add("Writer", "Escreva", Vec::new());
    let started = fixture
        .orchestration
        .start_worker(
            LEAD_SESSION,
            &writer.id,
            HarnessId::Claude,
            WorkerWorktree::Current,
            LaunchSetup::default(),
        )
        .expect("the worker starts");
    fixture
        .host
        .set_status(&started.session_id, AgentStatus::NeedsYou);

    let first = fixture
        .orchestration
        .check(LEAD_SESSION, Duration::ZERO)
        .expect("the lead checks");
    assert_eq!(first.messages.len(), 1);
    assert_eq!(first.messages[0].kind, MessageKind::NeedsYou);

    let again = fixture
        .orchestration
        .check(LEAD_SESSION, Duration::ZERO)
        .expect("the lead checks again");
    assert!(again.messages.is_empty(), "the same wait is said once");

    fixture
        .orchestration
        .reply_worker(LEAD_SESSION, &writer.id, "sim, pode seguir")
        .expect("the reply is typed");
    assert_eq!(
        fixture.host.sent(),
        vec![(started.session_id.clone(), "sim, pode seguir".to_owned())]
    );
    assert!(matches!(
        fixture
            .orchestration
            .reply_worker(LEAD_SESSION, &writer.id, "uma\nlinha"),
        Err(OrchestrationError::InvalidText(_))
    ));
}

#[test]
fn a_turn_that_ends_without_a_report_is_said_once_and_the_worker_keeps_running() {
    let fixture = fixture();
    let asker = fixture.add("Asker", "Pergunte a cor e espere", Vec::new());
    let started = fixture
        .orchestration
        .start_worker(
            LEAD_SESSION,
            &asker.id,
            HarnessId::Claude,
            WorkerWorktree::Current,
            LaunchSetup::default(),
        )
        .expect("the worker starts");
    fixture
        .host
        .set_status(&started.session_id, AgentStatus::Done);

    let first = fixture
        .orchestration
        .check(LEAD_SESSION, Duration::ZERO)
        .expect("the lead checks");
    assert_eq!(first.messages.len(), 1);
    assert_eq!(first.messages[0].kind, MessageKind::NoReport);
    let member = fixture.member(&asker.id);
    assert_eq!(
        member.task_state,
        Some(TaskState::Running),
        "the task is not failed for a turn"
    );
    assert_eq!(member.dispatch_state, Some(DispatchState::Running));

    assert!(
        fixture
            .orchestration
            .check(LEAD_SESSION, Duration::ZERO)
            .expect("the lead checks again")
            .messages
            .is_empty(),
        "the same turn is said once"
    );
}

#[test]
fn a_finished_worker_closes_when_the_setting_says_so() {
    let fixture = fixture_with(FakeHost::releasing());
    let writer = fixture.add("Writer", "Escreva", Vec::new());
    let started = fixture
        .orchestration
        .start_worker(
            LEAD_SESSION,
            &writer.id,
            HarnessId::Claude,
            WorkerWorktree::Current,
            LaunchSetup::default(),
        )
        .expect("the worker starts");
    fixture
        .orchestration
        .report(&started.session_id, Outcome::Done, "Pronto")
        .expect("it reports");

    fixture
        .orchestration
        .check(LEAD_SESSION, Duration::ZERO)
        .expect("the lead hears it");

    assert_eq!(fixture.host.stopped(), vec![started.session_id]);
    assert_eq!(
        fixture.member(&writer.id).dispatch_state,
        Some(DispatchState::Stopped)
    );
}

#[test]
fn a_bot_orchestrates_its_members_and_its_workers_wake_the_chat_that_started_them() {
    let fixture = fixture();
    let bot = fixture
        .team
        .add_member(NewTeamMember::Bot {
            bot_id: "docs".to_owned(),
            role: "Docs".to_owned(),
            worktree: WORKTREE.to_owned(),
        })
        .expect("the bot joins");
    assert_eq!(bot.kind, TeamMemberKind::Bot);
    let bot_session = "bot:docs:thread-1";

    let writer = fixture
        .orchestration
        .add_member(
            bot_session,
            None,
            "Writer",
            "Escreva a página",
            "",
            Vec::new(),
        )
        .expect("the bot adds a member");
    let started = fixture
        .orchestration
        .start_worker(
            bot_session,
            &writer.id,
            HarnessId::Claude,
            WorkerWorktree::Current,
            LaunchSetup::default(),
        )
        .expect("the bot starts a worker");
    fixture
        .orchestration
        .report(&started.session_id, Outcome::Failed, "sem acesso")
        .expect("the worker reports");

    assert_eq!(fixture.host.notified(), vec![bot_session.to_owned()]);
    let rows = fixture.orchestration.workers_of_bot("docs").expect("rows");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].member_id, writer.id);
    assert_eq!(rows[0].task_state, TaskState::Failed);
}

#[test]
fn nothing_works_while_orchestration_is_off() {
    let fixture = fixture();
    let writer = fixture.add("Writer", "Escreva", Vec::new());

    struct Off;
    impl WorkerHost for Off {
        fn settings(&self) -> OrchestrationConfig {
            OrchestrationConfig {
                enabled: false,
                ..OrchestrationConfig::default()
            }
        }
        fn new_worktree(&self, _from: &str, _name: &str) -> Result<String, String> {
            Err("off".to_owned())
        }
        fn prompt_file(&self, _text: &str) -> Result<String, String> {
            Err("off".to_owned())
        }
        fn launch_line(
            &self,
            _harness: HarnessId,
            _unattended: bool,
            _effort: Option<AgentEffort>,
        ) -> String {
            String::new()
        }
        fn start(&self, _worktree: &str, _command: &str) -> Result<String, String> {
            Err("off".to_owned())
        }
        fn running(&self, _session: &str) -> bool {
            false
        }
        fn status(&self, _session: &str) -> Option<AgentStatus> {
            None
        }
        fn output(&self, _session: &str) -> Result<String, String> {
            Err("off".to_owned())
        }
        fn stop(&self, _session: &str) {}
    }
    let off = Orchestration::new(Arc::clone(&fixture.team), Arc::new(Off));

    assert_eq!(
        off.start_worker(
            LEAD_SESSION,
            &writer.id,
            HarnessId::Claude,
            WorkerWorktree::Current,
            LaunchSetup::default()
        ),
        Err(OrchestrationError::Disabled)
    );
}

#[test]
fn dependencies_cannot_loop_and_cannot_cross_worktrees() {
    let fixture = fixture();
    let first = fixture.add("First", "Um", Vec::new());
    let second = fixture.add("Second", "Dois", vec![first.id.clone()]);

    assert!(
        matches!(
            fixture
                .team
                .set_dependencies(&first.id, std::slice::from_ref(&second.id)),
            Err(TeamError::DependencyCycle)
        ),
        "a task cannot wait for the task that waits for it"
    );
    assert!(
        matches!(
            fixture
                .team
                .set_dependencies(&first.id, std::slice::from_ref(&first.id)),
            Err(TeamError::DependencyCycle)
        ),
        "a task cannot wait for itself"
    );

    let elsewhere = fixture
        .team
        .add_member(NewTeamMember::Agent {
            role: "Elsewhere".to_owned(),
            harness: None,
            parent_id: None,
            orchestrator: false,
            task: String::new(),
            acceptance: String::new(),
            depends_on: Vec::new(),
            worktree: OTHER_WORKTREE.to_owned(),
            session_id: None,
            key: None,
            start_when_released: false,
        })
        .expect("a member of another worktree");
    assert!(
        matches!(
            fixture
                .team
                .set_dependencies(&first.id, std::slice::from_ref(&elsewhere.id)),
            Err(TeamError::DependencyOtherWorktree)
        ),
        "a task waits only for tasks of its own worktree"
    );
}

/// Starts the worker of `member` in the team's own worktree and lets it report `done`.
fn finished_worker(fixture: &Fixture, member: &TeamMember) -> String {
    let started = fixture
        .orchestration
        .start_worker(
            LEAD_SESSION,
            &member.id,
            HarnessId::Claude,
            WorkerWorktree::Current,
            LaunchSetup::default(),
        )
        .expect("the worker starts");
    fixture
        .orchestration
        .report(&started.session_id, Outcome::Done, "pronto")
        .expect("the worker reports");
    started.session_id
}

#[test]
fn a_finished_worker_takes_its_next_task_in_the_same_session() {
    let fixture = fixture();
    let writer = fixture.add("Writer", "Escreva o guia", Vec::new());
    let session = finished_worker(&fixture, &writer);

    let given = fixture
        .orchestration
        .assign(LEAD_SESSION, "writer", "Revise o guia", "sem erros")
        .expect("the task is given by role");

    assert_eq!(given, Assignment::Sent);
    assert_eq!(fixture.host.started().len(), 1, "no second terminal");
    assert_eq!(fixture.host.sent().len(), 1);
    assert_eq!(fixture.host.sent()[0].0, session);
    assert!(
        fixture.host.prompts()[1].contains("Revise o guia"),
        "the new task is in the file the worker reads"
    );
    let writer = fixture.member(&writer.id);
    assert_eq!(writer.task, "Revise o guia");
    assert_eq!(writer.task_state, Some(TaskState::Running));
    assert_eq!(writer.session_id.as_deref(), Some(session.as_str()));
}

#[test]
fn a_member_still_working_is_not_given_another_task() {
    let fixture = fixture();
    let writer = fixture.add("Writer", "Escreva", Vec::new());
    fixture
        .orchestration
        .start_worker(
            LEAD_SESSION,
            &writer.id,
            HarnessId::Claude,
            WorkerWorktree::Current,
            LaunchSetup::default(),
        )
        .expect("the worker starts");

    assert_eq!(
        fixture
            .orchestration
            .assign(LEAD_SESSION, &writer.id, "Outra", ""),
        Err(OrchestrationError::StillWorking)
    );
    assert_eq!(fixture.member(&writer.id).task, "Escreva");
}

#[test]
fn a_task_for_a_member_whose_worker_is_gone_waits_to_be_started() {
    let fixture = fixture();
    let writer = fixture.add("Writer", "Escreva", Vec::new());
    let session = finished_worker(&fixture, &writer);
    fixture.host.end(&session);

    let given = fixture
        .orchestration
        .assign(LEAD_SESSION, &writer.id, "Revise", "")
        .expect("the task is given");

    assert_eq!(given, Assignment::Pending);
    assert!(fixture.host.sent().is_empty());
    assert_eq!(
        fixture.member(&writer.id).task_state,
        Some(TaskState::Pending)
    );
    fixture
        .orchestration
        .start_worker(
            LEAD_SESSION,
            &writer.id,
            HarnessId::Codex,
            WorkerWorktree::Current,
            LaunchSetup::default(),
        )
        .expect("a worker starts for the new task");
}

#[test]
fn a_member_is_named_by_id_or_by_role_and_a_shared_role_must_be_told_apart() {
    let fixture = fixture();
    let first = fixture.add("Tester", "Teste A", Vec::new());
    assert_eq!(
        fixture
            .orchestration
            .assign(LEAD_SESSION, "TESTER", "Teste B", "")
            .expect("one member has that role"),
        Assignment::Pending
    );
    assert_eq!(fixture.member(&first.id).task, "Teste B");

    let second = fixture.add("Tester", "Teste C", Vec::new());
    assert!(matches!(
        fixture
            .orchestration
            .assign(LEAD_SESSION, "tester", "Teste D", ""),
        Err(OrchestrationError::AmbiguousMember(_, _))
    ));
    fixture
        .orchestration
        .assign(LEAD_SESSION, &second.id, "Teste D", "")
        .expect("the id tells them apart");
}

#[test]
fn a_batch_of_follow_ups_reaches_each_running_worker() {
    let fixture = fixture();
    let a = fixture.add("A", "Tarefa A", Vec::new());
    let b = fixture.add("B", "Tarefa B", Vec::new());
    let start = |member: &TeamMember| {
        fixture
            .orchestration
            .start_worker(
                LEAD_SESSION,
                &member.id,
                HarnessId::Claude,
                WorkerWorktree::Current,
                LaunchSetup::default(),
            )
            .expect("the worker starts")
            .session_id
    };
    let (sa, sb) = (start(&a), start(&b));

    fixture
        .orchestration
        .reply_worker(LEAD_SESSION, "a", "um")
        .expect("by role");
    fixture
        .orchestration
        .reply_worker(LEAD_SESSION, &b.id, "dois")
        .expect("by id");

    assert_eq!(
        fixture.host.sent(),
        vec![(sa, "um".to_owned()), (sb, "dois".to_owned())]
    );
}

#[test]
fn replacing_the_agent_keeps_the_worktree_and_the_task() {
    let fixture = fixture();
    let writer = fixture.add("Writer", "Escreva o guia", Vec::new());
    let first = fixture
        .orchestration
        .start_worker(
            LEAD_SESSION,
            &writer.id,
            HarnessId::Claude,
            WorkerWorktree::New,
            LaunchSetup::default(),
        )
        .expect("the worker starts in a worktree of its own");

    let second = fixture
        .orchestration
        .replace_worker(
            LEAD_SESSION,
            "writer",
            HarnessId::Codex,
            LaunchSetup::default(),
        )
        .expect("the agent is replaced");

    assert_eq!(fixture.host.stopped(), vec![first.session_id.clone()]);
    assert_eq!(second.worktree_path, first.worktree_path);
    assert_ne!(second.session_id, first.session_id);
    assert!(
        fixture.host.prompts()[1].contains("Another agent began this task"),
        "the new agent is told what happened"
    );
    let writer = fixture.member(&writer.id);
    assert_eq!(writer.harness, Some(HarnessId::Codex));
    assert_eq!(writer.task_state, Some(TaskState::Running));
    assert_eq!(writer.dispatch_state, Some(DispatchState::Running));
}

#[test]
fn a_replacement_that_is_refused_leaves_the_old_worker_running() {
    let fixture = fixture();
    let writer = fixture.add("Writer", "Escreva", Vec::new());
    fixture
        .orchestration
        .start_worker(
            LEAD_SESSION,
            &writer.id,
            HarnessId::Claude,
            WorkerWorktree::Current,
            LaunchSetup::default(),
        )
        .expect("the worker starts");

    assert_eq!(
        fixture.orchestration.replace_worker(
            LEAD_SESSION,
            &writer.id,
            HarnessId::Opencode,
            LaunchSetup {
                model: Some("gpt-5".to_owned()),
                effort: None,
            }
        ),
        Err(OrchestrationError::ModelNotSupported)
    );
    assert!(fixture.host.stopped().is_empty());
    assert_eq!(
        fixture.member(&writer.id).dispatch_state,
        Some(DispatchState::Running)
    );
}

#[test]
fn a_worker_reads_the_prompt_of_its_saved_role() {
    let fixture = fixture();
    fixture
        .orchestration
        .save_role(LEAD_SESSION, "Reviewer", "Review only. Never edit files.")
        .expect("the role is saved");
    let reviewer = fixture.add("reviewer", "Revise o PR", Vec::new());
    fixture
        .orchestration
        .start_worker(
            LEAD_SESSION,
            &reviewer.id,
            HarnessId::Claude,
            WorkerWorktree::Current,
            LaunchSetup::default(),
        )
        .expect("the worker starts");

    let prompt = fixture.host.prompts().remove(0);
    assert!(prompt.starts_with("Your role:\nReview only. Never edit files."));
    assert!(prompt.contains("Revise o PR"));

    fixture
        .orchestration
        .save_role(LEAD_SESSION, "REVIEWER", "Review and approve.")
        .expect("the same name replaces the prompt");
    let roles = fixture.orchestration.roles(LEAD_SESSION).expect("roles");
    assert_eq!(roles.len(), 1);
    assert_eq!(roles[0].1, "Review and approve.");
    assert!(
        fixture
            .orchestration
            .delete_role(LEAD_SESSION, "reviewer")
            .expect("deleted")
    );
}

#[test]
fn the_team_shares_notes_that_a_worker_writes_but_only_the_orchestrator_removes() {
    let fixture = fixture();
    let writer = fixture.add("Writer", "Escreva", Vec::new());
    let session = finished_worker(&fixture, &writer);
    let orchestration = &fixture.orchestration;

    let tree = orchestration
        .notes_worktree(&session, None)
        .expect("a worker is in the team's worktree");
    assert_eq!(tree, WORKTREE);
    orchestration
        .write_note(&tree, "plano", "passo 1\npasso 2")
        .expect("a worker writes");
    orchestration
        .edit_note(&tree, "PLANO", "passo 2", "passo dois")
        .expect("the name ignores case");

    assert_eq!(
        orchestration.read_note(&tree, "plano").expect("read"),
        "passo 1\npasso dois"
    );
    assert_eq!(orchestration.note_names(&tree).expect("names"), ["plano"]);
    assert!(
        orchestration
            .edit_note(&tree, "plano", "inexistente", "x")
            .is_err(),
        "the text to replace must be there"
    );
    assert_eq!(
        orchestration.delete_note(&session, &tree, "plano"),
        Err(OrchestrationError::WorkerCannotCoordinate)
    );
    orchestration
        .delete_note(LEAD_SESSION, &tree, "plano")
        .expect("the orchestrator removes it");
    assert!(orchestration.read_note(&tree, "plano").is_err());
    assert!(orchestration.notes_worktree("stranger", None).is_err());
}

#[test]
fn only_a_stopped_worker_with_a_worktree_of_its_own_is_landed() {
    let fixture = fixture();
    let writer = fixture.add("Writer", "Escreva", Vec::new());
    let started = fixture
        .orchestration
        .start_worker(
            LEAD_SESSION,
            &writer.id,
            HarnessId::Claude,
            WorkerWorktree::New,
            LaunchSetup::default(),
        )
        .expect("the worker starts");

    assert!(matches!(
        fixture.orchestration.land_worker(LEAD_SESSION, &writer.id),
        Err(OrchestrationError::WorkerRunning(_))
    ));
    fixture
        .orchestration
        .stop_worker(LEAD_SESSION, &writer.id)
        .expect("stopped");

    fixture
        .orchestration
        .land_worker(LEAD_SESSION, &writer.id)
        .expect("landed");
    assert_eq!(
        fixture.host.landed(),
        vec![(started.worktree_path, WORKTREE.to_owned())]
    );

    let shared = fixture.add("Shared", "Trabalhe aqui", Vec::new());
    finished_worker(&fixture, &shared);
    assert!(
        fixture
            .orchestration
            .land_worker(LEAD_SESSION, &shared.id)
            .is_err(),
        "a worker in the team's own worktree has nothing to land"
    );
}

/// A worker started under the lead, with the session the host gave it.
fn started_worker(fixture: &Fixture, role: &str, task: &str) -> (TeamMember, String) {
    let member = fixture.add(role, task, Vec::new());
    let started = fixture
        .orchestration
        .start_worker(
            LEAD_SESSION,
            &member.id,
            HarnessId::Claude,
            WorkerWorktree::Current,
            LaunchSetup::default(),
        )
        .expect("the worker starts");
    (fixture.member(&member.id), started.session_id)
}

/// Waits, for up to five seconds, until `condition` holds: the other side of a test has to reach its step.
fn wait_for(condition: impl Fn() -> bool) {
    let until = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(Instant::now() < until, "the step did not come in time");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// A member of a batch, named by `key`, that waits for the keys in `depends_on` and starts now when `start`.
fn batch_member(key: &str, role: &str, depends_on: &[&str], start: bool) -> NewMember {
    NewMember {
        key: Some(key.to_owned()),
        role: role.to_owned(),
        task: format!("Faça {role}"),
        acceptance: String::new(),
        depends_on: depends_on.iter().map(|key| (*key).to_owned()).collect(),
        start,
        agent: start.then_some(HarnessId::Claude),
    }
}

#[test]
fn a_question_is_typed_into_an_idle_worker_and_its_report_is_the_answer() {
    let fixture = fixture();
    let (writer, session) = started_worker(&fixture, "Writer", "Escreva o guia");
    fixture.host.set_status(&session, AgentStatus::Idle);

    std::thread::scope(|scope| {
        let asking = scope.spawn(|| {
            fixture.orchestration.ask_worker(
                LEAD_SESSION,
                &writer.id,
                "Qual o título?",
                Duration::from_secs(5),
            )
        });
        wait_for(|| !fixture.host.sent().is_empty());
        fixture
            .orchestration
            .report(&session, Outcome::Done, "O título é Guia.")
            .expect("the worker reports");
        let answer = asking
            .join()
            .expect("the ask does not panic")
            .expect("the worker answers");

        assert_eq!(answer.report.as_deref(), Some("O título é Guia."));
        assert_eq!(answer.output, format!("the screen of {session}"));
        assert!(answer.running);
    });
    assert_eq!(
        fixture.host.sent(),
        vec![(session.clone(), "Qual o título?".to_owned())]
    );
    let exchanges = fixture
        .team
        .exchanges_of(&fixture.lead.id)
        .expect("the exchanges");
    assert!(exchanges.iter().any(|exchange| {
        exchange.kind == MessageKind::Question
            && exchange.from_member_id == fixture.lead.id
            && exchange.to_member_id == writer.id
            && exchange.body == "Qual o título?"
    }));
    assert!(exchanges.iter().any(|exchange| {
        exchange.kind == MessageKind::Answer
            && exchange.from_member_id == writer.id
            && exchange.to_member_id == fixture.lead.id
    }));
}

#[test]
fn a_question_waits_until_a_busy_worker_is_idle() {
    let fixture = fixture();
    let (writer, session) = started_worker(&fixture, "Writer", "Escreva o guia");
    fixture.host.set_status(&session, AgentStatus::Working);

    std::thread::scope(|scope| {
        let asking = scope.spawn(|| {
            fixture.orchestration.ask_worker(
                LEAD_SESSION,
                &writer.id,
                "Pode revisar?",
                Duration::from_secs(5),
            )
        });
        std::thread::sleep(Duration::from_millis(300));
        assert!(
            fixture.host.sent().is_empty(),
            "nothing is typed into a worker that is busy"
        );
        fixture.host.set_status(&session, AgentStatus::Idle);
        wait_for(|| !fixture.host.sent().is_empty());
        fixture
            .orchestration
            .report(&session, Outcome::Done, "Revisado.")
            .expect("the worker reports");
        let answer = asking
            .join()
            .expect("the ask does not panic")
            .expect("the worker answers");
        assert_eq!(answer.report.as_deref(), Some("Revisado."));
    });
}

#[test]
fn a_draft_in_the_worker_input_holds_the_question_back() {
    let fixture = fixture();
    let (writer, session) = started_worker(&fixture, "Writer", "Escreva o guia");
    fixture.host.set_status(&session, AgentStatus::Idle);
    fixture.host.set_draft(&session, true);

    assert_eq!(
        fixture.orchestration.ask_worker(
            LEAD_SESSION,
            &writer.id,
            "Oi?",
            Duration::from_millis(300)
        ),
        Err(OrchestrationError::AskNotTyped)
    );
    assert!(
        fixture.host.sent().is_empty(),
        "the question is not mixed into the person's draft"
    );
}

#[test]
fn a_worker_that_does_not_answer_in_time_is_reported_as_such() {
    let fixture = fixture();
    let (writer, session) = started_worker(&fixture, "Writer", "Escreva o guia");
    fixture.host.set_status(&session, AgentStatus::Idle);

    assert_eq!(
        fixture.orchestration.ask_worker(
            LEAD_SESSION,
            &writer.id,
            "Oi?",
            Duration::from_millis(300)
        ),
        Err(OrchestrationError::AskNoAnswer)
    );
    assert_eq!(
        fixture.host.sent().len(),
        1,
        "the question was typed before the wait ran out"
    );
}

#[test]
fn a_question_to_a_stopped_worker_is_refused_before_anything_is_typed() {
    let fixture = fixture();
    let (writer, session) = started_worker(&fixture, "Writer", "Escreva o guia");
    fixture.host.stop(&session);

    assert!(matches!(
        fixture
            .orchestration
            .ask_worker(LEAD_SESSION, &writer.id, "Oi?", Duration::from_secs(1)),
        Err(OrchestrationError::Host(_))
    ));
    assert!(fixture.host.sent().is_empty());
}

#[test]
fn a_refused_question_does_not_end_the_wait_of_the_one_in_progress() {
    let fixture = fixture();
    let (writer, writer_session) = started_worker(&fixture, "Writer", "Escreva o guia");
    let (reviewer, reviewer_session) = started_worker(&fixture, "Reviewer", "Revise o guia");
    fixture
        .host
        .set_status(&reviewer_session, AgentStatus::Idle);

    std::thread::scope(|scope| {
        let first = scope.spawn(|| {
            fixture.orchestration.member_ask(
                &writer_session,
                &reviewer.id,
                "Está pronto?",
                Duration::from_secs(5),
            )
        });
        wait_for(|| !fixture.host.sent().is_empty());
        assert_eq!(
            fixture.orchestration.member_ask(
                &writer_session,
                &reviewer.id,
                "Está pronto?",
                Duration::from_millis(300)
            ),
            Err(OrchestrationError::AskNotTyped),
            "the second question waits behind the first"
        );
        // The reviewer may not ask the writer back while the writer still waits for it.
        assert_eq!(
            fixture.orchestration.member_ask(
                &reviewer_session,
                &writer.id,
                "E você?",
                Duration::from_millis(300)
            ),
            Err(OrchestrationError::AskCycle)
        );
        fixture
            .orchestration
            .report(&reviewer_session, Outcome::Done, "Pronto.")
            .expect("the reviewer reports");
        first
            .join()
            .expect("the ask does not panic")
            .expect("the first question is answered");
    });
}

#[test]
fn a_member_asks_a_teammate_by_role_and_the_exchange_stays_for_the_orchestrator() {
    let fixture = fixture();
    let (writer, writer_session) = started_worker(&fixture, "Writer", "Escreva o guia");
    let (reviewer, reviewer_session) = started_worker(&fixture, "Reviewer", "Revise o guia");
    fixture
        .host
        .set_status(&reviewer_session, AgentStatus::Idle);

    std::thread::scope(|scope| {
        let asking = scope.spawn(|| {
            fixture.orchestration.member_ask(
                &writer_session,
                "reviewer",
                "Posso fechar?",
                Duration::from_secs(5),
            )
        });
        wait_for(|| !fixture.host.sent().is_empty());
        fixture
            .orchestration
            .report(&reviewer_session, Outcome::Done, "Pode fechar.")
            .expect("the reviewer reports");
        let answer = asking
            .join()
            .expect("the ask does not panic")
            .expect("the reviewer answers");
        assert_eq!(answer.report.as_deref(), Some("Pode fechar."));
    });
    assert_eq!(
        fixture.host.sent(),
        vec![(reviewer_session.clone(), "Posso fechar?".to_owned())]
    );

    let exchanges = fixture
        .team
        .exchanges_of(&writer.id)
        .expect("the exchanges");
    assert!(exchanges.iter().any(|exchange| {
        exchange.kind == MessageKind::Question
            && exchange.from_member_id == writer.id
            && exchange.to_member_id == reviewer.id
    }));
    assert!(exchanges.iter().any(|exchange| {
        exchange.kind == MessageKind::Answer
            && exchange.from_member_id == reviewer.id
            && exchange.to_member_id == writer.id
    }));

    let check = fixture
        .orchestration
        .check(LEAD_SESSION, Duration::ZERO)
        .expect("the lead checks");
    assert!(
        check.messages.iter().any(|message| {
            message.kind == MessageKind::Question && message.member_id == writer.id
        }),
        "the lead's check shows who asked"
    );
}

#[test]
fn a_member_asks_only_its_teammates_and_never_itself() {
    let fixture = fixture();
    let (writer, writer_session) = started_worker(&fixture, "Writer", "Escreva o guia");
    let other = fixture
        .team
        .add_member(NewTeamMember::Agent {
            role: "Other".to_owned(),
            harness: None,
            parent_id: None,
            orchestrator: false,
            task: "Outra tarefa".to_owned(),
            acceptance: String::new(),
            depends_on: Vec::new(),
            worktree: OTHER_WORKTREE.to_owned(),
            session_id: None,
            key: None,
            start_when_released: false,
        })
        .expect("a member of another team");

    assert_eq!(
        fixture.orchestration.member_ask(
            &writer_session,
            &other.id,
            "Oi?",
            Duration::from_millis(100)
        ),
        Err(OrchestrationError::NotInYourTeam(other.id.clone()))
    );
    assert_eq!(
        fixture.orchestration.member_ask(
            &writer_session,
            &writer.id,
            "Oi?",
            Duration::from_millis(100)
        ),
        Err(OrchestrationError::AskSelf)
    );
    assert!(fixture.host.sent().is_empty());
}

#[test]
fn a_member_cannot_ask_while_orchestration_is_off() {
    let fixture = fixture_with(FakeHost::disabled());

    assert_eq!(
        fixture.orchestration.member_ask(
            "session-1",
            &fixture.lead.id,
            "Oi?",
            Duration::from_millis(100)
        ),
        Err(OrchestrationError::Disabled)
    );
}

#[test]
fn a_batch_is_written_whole_or_not_at_all() {
    let fixture = fixture();

    assert_eq!(
        fixture
            .orchestration
            .add_members(LEAD_SESSION, None, Vec::new())
            .map(|_| ()),
        Err(OrchestrationError::InvalidText(
            "a member list needs at least one member".to_owned()
        ))
    );
    assert!(matches!(
        fixture.orchestration.add_members(
            LEAD_SESSION,
            None,
            vec![
                batch_member("a", "Writer", &[], false),
                NewMember {
                    task: "  ".to_owned(),
                    ..batch_member("b", "Reviewer", &[], false)
                },
            ],
        ),
        Err(OrchestrationError::InvalidText(_))
    ));
    assert!(
        fixture
            .orchestration
            .add_members(
                LEAD_SESSION,
                None,
                vec![
                    batch_member("a", "Writer", &[], false),
                    batch_member("a", "Reviewer", &[], false),
                ],
            )
            .is_err(),
        "a key may name one member only"
    );
    assert!(matches!(
        fixture.orchestration.add_members(
            LEAD_SESSION,
            None,
            vec![NewMember {
                start: true,
                agent: None,
                ..batch_member("a", "Writer", &[], false)
            }],
        ),
        Err(OrchestrationError::InvalidText(_))
    ));
    assert_eq!(
        fixture.team.members().expect("members").len(),
        1,
        "only the lead is in the team"
    );
}

#[test]
fn a_batch_member_waits_for_the_key_it_names_and_starts_when_that_member_reports() {
    let fixture = fixture();
    let added = fixture
        .orchestration
        .add_members(
            LEAD_SESSION,
            None,
            vec![
                batch_member("writer", "Writer", &[], true),
                batch_member("reviewer", "Reviewer", &["writer"], true),
            ],
        )
        .expect("the batch is added");
    let [writer, reviewer] = added.as_slice() else {
        panic!("the batch has two members");
    };

    assert_eq!(
        reviewer.member.depends_on,
        vec![writer.member.id.clone()],
        "the key is read as the member's id"
    );
    assert!(reviewer.member.start_when_released);
    assert!(
        matches!(writer.started, Some(Ok(_))),
        "a member with nothing to wait for starts now"
    );
    assert_eq!(
        reviewer.started, None,
        "a member that waits does not start yet"
    );
    assert_eq!(fixture.host.started().len(), 1);

    let writer_session = fixture
        .member(&writer.member.id)
        .session_id
        .expect("the writer has a session");
    fixture
        .orchestration
        .report(&writer_session, Outcome::Done, "Escrito.")
        .expect("the writer reports");
    assert_eq!(
        fixture.host.started().len(),
        2,
        "the reviewer starts once the writer reports"
    );
    assert!(fixture.member(&reviewer.member.id).session_id.is_some());
}

#[test]
fn the_effort_a_start_names_reaches_the_agent_it_launches() {
    let fixture = fixture();
    let writer = fixture.add("Writer", "Escreva o guia", Vec::new());

    fixture
        .orchestration
        .start_worker(
            LEAD_SESSION,
            &writer.id,
            HarnessId::Claude,
            WorkerWorktree::Current,
            LaunchSetup {
                model: None,
                effort: Some(AgentEffort::High),
            },
        )
        .expect("the worker starts");

    assert_eq!(fixture.host.efforts(), vec![Some(AgentEffort::High)]);
}

#[test]
fn the_effort_a_replacement_names_reaches_the_agent_it_launches() {
    let fixture = fixture();
    let writer = fixture.add("Writer", "Escreva o guia", Vec::new());
    fixture
        .orchestration
        .start_worker(
            LEAD_SESSION,
            &writer.id,
            HarnessId::Claude,
            WorkerWorktree::Current,
            LaunchSetup::default(),
        )
        .expect("the worker starts");

    fixture
        .orchestration
        .replace_worker(
            LEAD_SESSION,
            "writer",
            HarnessId::Codex,
            LaunchSetup {
                model: None,
                effort: Some(AgentEffort::Max),
            },
        )
        .expect("the agent is replaced");

    assert_eq!(fixture.host.efforts(), vec![None, Some(AgentEffort::Max)]);
}

#[test]
fn a_worker_is_told_to_ask_its_teammates_with_the_member_ask_tool() {
    let fixture = fixture();
    let writer = fixture.add("Writer", "Escreva o guia", Vec::new());
    fixture
        .orchestration
        .start_worker(
            LEAD_SESSION,
            &writer.id,
            HarnessId::Claude,
            WorkerWorktree::Current,
            LaunchSetup::default(),
        )
        .expect("the worker starts");

    let prompt = fixture.host.prompts().pop().expect("the prompt is written");
    assert!(
        prompt.contains("lynx_member_ask"),
        "a worker learns how to ask a teammate"
    );
}
