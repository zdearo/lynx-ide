//! The coordination rules, tested with a host in memory: no terminal, no worktree, no settings file.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::agents::activity::AgentStatus;
use crate::agents::harness::HarnessId;
use crate::config::app_config::OrchestrationConfig;
use crate::persistence::AppDatabase;
use crate::team::team_error::TeamError;
use crate::team::team_model::{
    DispatchState, MessageKind, NewTeamMember, Outcome, TaskState, TeamMember, TeamMemberKind,
};
use crate::team::team_service::TeamService;

use super::service::{Orchestration, OrchestrationError, WorkerHost, WorkerWorktree};

/// The terminals and worktrees of a test: what it started, stopped and was asked to type.
#[derive(Default)]
pub struct FakeHost {
    /// Whether orchestration asks for a worker's reports to close the worker, as the setting says.
    pub release: bool,
    running: Mutex<Vec<String>>,
    statuses: Mutex<HashMap<String, AgentStatus>>,
    started: Mutex<Vec<(String, String)>>,
    stopped: Mutex<Vec<String>>,
    notified: Mutex<Vec<String>>,
    sent: Mutex<Vec<(String, String)>>,
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
            enabled: true,
            release_finished_workers: self.release,
        }
    }

    fn new_worktree(&self, from: &str, name: &str) -> Result<String, String> {
        Ok(format!("{from}/{name}"))
    }

    fn prompt_file(&self, _text: &str) -> Result<String, String> {
        Ok("/prompts/task.md".to_owned())
    }

    fn launch_line(&self, harness: HarnessId, _unattended: bool) -> String {
        format!("agent-{}", crate::agents::harness::harness_name(harness))
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

    fn send(&self, session: &str, text: &str) -> Result<(), String> {
        self.sent
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push((session.to_owned(), text.to_owned()));
        Ok(())
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
pub const WORKTREE: &str = "/repo/app";

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
            None
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
            None,
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
            .start_worker(LEAD_SESSION, &tests.id, HarnessId::Claude, WorkerWorktree::Current, None),
        Err(OrchestrationError::WaitingFor(names)) if names == "Researcher"
    ));

    let started = fixture
        .orchestration
        .start_worker(
            LEAD_SESSION,
            &research.id,
            HarnessId::Claude,
            WorkerWorktree::Current,
            None,
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
            None,
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
            None,
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
            None,
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
            None,
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
            None,
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
            None,
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
            None,
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
            None,
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
            None,
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
            None,
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
        fn launch_line(&self, _harness: HarnessId, _unattended: bool) -> String {
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
            None
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
            worktree: "/repo/docs".to_owned(),
            session_id: None,
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
