//! The orchestration rules, with a host in memory: the settings, dependencies, reports, checks,
//! and the refusals.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::agents::activity::AgentStatus;
use crate::agents::harness::{HarnessId, harness_name};
use crate::config::app_config::OrchestrationConfig;
use crate::orchestration::model::{
    Dispatch, MessageKind, Outcome, Run, Task, TaskState, WorkerWorktree,
};
use crate::orchestration::service::{
    Orchestration, OrchestrationError, WorkerHost, quote_for_shell,
};
use crate::persistence::AppDatabase;

/// Settings with orchestration on, the way a user who turned it on has them.
fn enabled() -> OrchestrationConfig {
    OrchestrationConfig {
        enabled: true,
        ..OrchestrationConfig::default()
    }
}

/// Terminals and worktrees in memory: a session belongs to a project when it is listed.
#[derive(Default)]
struct FakeHost {
    settings: Mutex<OrchestrationConfig>,
    projects: HashMap<String, String>,
    running: Mutex<HashSet<String>>,
    statuses: Mutex<HashMap<String, AgentStatus>>,
    started: Mutex<Vec<(String, String)>>,
    prompts: Mutex<Vec<String>>,
    output: Mutex<String>,
    count: Mutex<u32>,
    stopped: Mutex<Vec<String>>,
    /// Why starting a worker is refused, when it is.
    refuse: Mutex<Option<String>>,
    /// What was typed into the workers, as (session, text).
    sent: Mutex<Vec<(String, String)>>,
}

impl FakeHost {
    /// Orchestration on, and no project.
    fn new() -> Self {
        Self {
            settings: Mutex::new(enabled()),
            ..Self::default()
        }
    }

    /// Orchestration on, and `session` works in the project `/repo/app`.
    fn with_project(session: &str) -> Self {
        let mut host = Self::new();
        host.projects
            .insert(session.to_owned(), "/repo/app".to_owned());
        host
    }

    fn set_settings(&self, settings: OrchestrationConfig) {
        *self.settings.lock().expect("lock") = settings;
    }

    fn started_commands(&self) -> Vec<(String, String)> {
        self.started.lock().expect("lock").clone()
    }

    fn end(&self, session: &str) {
        self.running.lock().expect("lock").remove(session);
    }

    fn set_status(&self, session: &str, status: AgentStatus) {
        self.statuses
            .lock()
            .expect("lock")
            .insert(session.to_owned(), status);
    }
}

impl WorkerHost for FakeHost {
    fn settings(&self) -> OrchestrationConfig {
        self.settings.lock().expect("lock").clone()
    }

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

    fn launch_line(&self, harness: HarnessId, unattended: bool) -> String {
        let name = harness_name(harness);
        if unattended {
            format!("{name} --yolo")
        } else {
            name.to_owned()
        }
    }

    fn allow_start(&self, _coordinator: &str, worktree: &str) -> Result<(), String> {
        match self.refuse.lock().expect("lock").as_deref() {
            Some(reason) if worktree.starts_with("/repo") => Err(reason.to_owned()),
            _ => Ok(()),
        }
    }

    fn send(&self, session: &str, text: &str) -> Result<(), String> {
        self.sent
            .lock()
            .expect("lock")
            .push((session.to_owned(), text.to_owned()));
        Ok(())
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

    fn status(&self, session: &str) -> Option<AgentStatus> {
        self.statuses.lock().expect("lock").get(session).copied()
    }

    /// A session that has ended has no screen to read.
    fn output(&self, session: &str) -> Result<String, String> {
        if self.running(session) {
            Ok(self.output.lock().expect("lock").clone())
        } else {
            Err("no terminal".to_owned())
        }
    }

    fn stop(&self, session: &str) {
        self.stopped.lock().expect("lock").push(session.to_owned());
        self.end(session);
    }
}

/// A database in a temporary folder, and a run the coordinator started.
struct Setup {
    orchestration: Orchestration,
    host: Arc<FakeHost>,
    run: Run,
    _dir: tempfile::TempDir,
}

fn setup(host: FakeHost) -> Setup {
    let dir = tempfile::tempdir().expect("temp dir");
    let database = Arc::new(AppDatabase::open(&dir.path().join("app.db")).expect("database"));
    let host = Arc::new(host);
    let orchestration = Orchestration::new(database, Arc::clone(&host) as Arc<dyn WorkerHost>);
    let run = orchestration
        .start_run("coordinator", "Audit")
        .expect("run");
    Setup {
        orchestration,
        host,
        run,
        _dir: dir,
    }
}

/// A coordinator in a project, with a run, and orchestration on.
fn project() -> Setup {
    setup(FakeHost::with_project("coordinator"))
}

fn task(setup: &Setup, spec: &str) -> Task {
    setup
        .orchestration
        .create_task("coordinator", spec, vec![])
        .expect("task")
}

fn start(setup: &Setup, task: &Task) -> Dispatch {
    setup
        .orchestration
        .start_worker(
            "coordinator",
            &task.id,
            HarnessId::Claude,
            WorkerWorktree::New,
            None,
        )
        .expect("start")
}

#[test]
fn a_run_starts_in_the_coordinators_project() {
    let setup = project();
    assert_eq!(setup.run.worktree_path, "/repo/app");
    assert_eq!(setup.run.title, "Audit");
}

#[test]
fn a_session_outside_a_project_cannot_start_a_run() {
    let dir = tempfile::tempdir().expect("temp dir");
    let database = Arc::new(AppDatabase::open(&dir.path().join("app.db")).expect("database"));
    let orchestration = Orchestration::new(database, Arc::new(FakeHost::new()));
    assert_eq!(
        orchestration.start_run("loose", "Audit").map(|_| ()),
        Err(OrchestrationError::NotInProject)
    );
}

#[test]
fn nothing_is_answered_while_orchestration_is_turned_off() {
    let setup = project();
    let task = task(&setup, "x");
    setup.host.set_settings(OrchestrationConfig::default());

    assert_eq!(
        setup
            .orchestration
            .create_task("coordinator", "y", vec![])
            .map(|_| ()),
        Err(OrchestrationError::Disabled)
    );
    assert_eq!(
        setup
            .orchestration
            .start_worker(
                "coordinator",
                &task.id,
                HarnessId::Claude,
                WorkerWorktree::New,
                None
            )
            .map(|_| ()),
        Err(OrchestrationError::Disabled)
    );
    assert_eq!(
        setup
            .orchestration
            .check("coordinator", Duration::ZERO)
            .map(|_| ()),
        Err(OrchestrationError::Disabled)
    );
    assert!(setup.host.started_commands().is_empty());
}

#[test]
fn a_worker_starts_in_its_own_worktree_with_its_task() {
    let setup = project();
    let task = task(&setup, "fix the parser");

    let worker = setup
        .orchestration
        .start_worker(
            "coordinator",
            &task.id,
            HarnessId::Claude,
            WorkerWorktree::New,
            None,
        )
        .expect("start");

    assert_eq!(
        worker.worktree_path,
        format!("/repo/app-worktrees/lynx-task-{}", &task.id[..8])
    );
    let prompts = setup.host.prompts.lock().expect("lock").clone();
    assert!(prompts[0].starts_with("fix the parser"));
    assert!(
        prompts[0].contains("lynx_report"),
        "a worker that reports through the tools is told how"
    );
    // The instruction is quoted the way the platform's shell takes it.
    let instruction = quote_for_shell(
        "Read /tmp/lynx-prompts/task.md and follow the instructions in it.",
        cfg!(windows),
    );
    assert_eq!(
        setup.host.started_commands()[0].1,
        format!("claude {instruction}")
    );
}

#[test]
fn a_worker_works_in_the_coordinators_worktree_when_the_coordinator_asks_for_it() {
    let setup = project();
    let task = task(&setup, "x");

    let worker = setup
        .orchestration
        .start_worker(
            "coordinator",
            &task.id,
            HarnessId::Claude,
            WorkerWorktree::Current,
            None,
        )
        .expect("start");

    assert_eq!(worker.worktree_path, "/repo/app");
    assert_eq!(setup.host.started_commands()[0].0, "/repo/app");
}

#[test]
fn a_worker_starts_the_agent_and_the_model_the_coordinator_names() {
    let setup = project();
    let one = task(&setup, "one");
    let two = task(&setup, "two");

    let codex = setup
        .orchestration
        .start_worker(
            "coordinator",
            &one.id,
            HarnessId::Codex,
            WorkerWorktree::New,
            Some("o3"),
        )
        .expect("start");
    setup
        .orchestration
        .start_worker(
            "coordinator",
            &two.id,
            HarnessId::Claude,
            WorkerWorktree::New,
            Some("claude-opus-5-5"),
        )
        .expect("start");

    assert_eq!(codex.harness, HarnessId::Codex);
    let commands = setup.host.started_commands();
    assert!(commands[0].1.starts_with("codex --model o3 "));
    assert!(commands[0].1.contains("Read /tmp/lynx-prompts/task.md"));
    assert!(commands[1].1.starts_with("claude --model claude-opus-5-5 "));
}

#[test]
fn a_model_is_plain_text_and_opencode_takes_none_at_start() {
    let setup = project();
    let task = task(&setup, "x");
    let start = |harness: HarnessId, model: &str| {
        setup
            .orchestration
            .start_worker(
                "coordinator",
                &task.id,
                harness,
                WorkerWorktree::New,
                Some(model),
            )
            .map(|_| ())
    };

    for model in ["opus; rm -rf ~", "a b", "", &"m".repeat(101)] {
        assert!(
            matches!(
                start(HarnessId::Claude, model),
                Err(OrchestrationError::InvalidText(_))
            ),
            "{model:?} is refused"
        );
    }
    assert_eq!(
        start(HarnessId::Opencode, "gpt-5.2"),
        Err(OrchestrationError::ModelNotSupported)
    );
    assert!(
        setup.host.started_commands().is_empty(),
        "a refused model starts no worker and makes no worktree"
    );
    assert!(start(HarnessId::Claude, "openai/gpt-5.2").is_ok());
}

#[test]
fn there_is_no_limit_on_the_workers_running_at_once() {
    let setup = project();
    let tasks: Vec<Task> = (0..5).map(|n| task(&setup, &format!("task {n}"))).collect();

    for task in &tasks {
        start(&setup, task);
    }

    assert_eq!(setup.host.started_commands().len(), 5);
}

#[test]
fn a_task_waits_for_the_tasks_it_depends_on() {
    let setup = project();
    let first = task(&setup, "first");
    let second = setup
        .orchestration
        .create_task("coordinator", "second", vec![first.id.clone()])
        .expect("task");

    assert_eq!(
        setup
            .orchestration
            .start_worker(
                "coordinator",
                &second.id,
                HarnessId::Claude,
                WorkerWorktree::New,
                None
            )
            .map(|_| ()),
        Err(OrchestrationError::WaitingFor(first.id.clone()))
    );

    setup
        .orchestration
        .database
        .set_task_state(&first.id, TaskState::Done)
        .expect("done");
    assert!(
        setup
            .orchestration
            .start_worker(
                "coordinator",
                &second.id,
                HarnessId::Claude,
                WorkerWorktree::New,
                None
            )
            .is_ok()
    );
}

#[test]
fn a_worker_cannot_start_runs_tasks_or_workers_or_hear_the_coordinators_check() {
    let setup = project();
    let task = task(&setup, "x");
    let worker = start(&setup, &task);
    let worker_session = worker.session_id.clone();

    assert_eq!(
        setup
            .orchestration
            .start_run(&worker_session, "nested")
            .map(|_| ()),
        Err(OrchestrationError::WorkerCannotCoordinate)
    );
    assert_eq!(
        setup
            .orchestration
            .create_task(&worker_session, "nested", vec![])
            .map(|_| ()),
        Err(OrchestrationError::WorkerCannotCoordinate)
    );
    assert_eq!(
        setup
            .orchestration
            .start_worker(
                &worker_session,
                &task.id,
                HarnessId::Claude,
                WorkerWorktree::New,
                None
            )
            .map(|_| ()),
        Err(OrchestrationError::WorkerCannotCoordinate)
    );
    assert_eq!(
        setup
            .orchestration
            .check(&worker_session, Duration::ZERO)
            .map(|_| ()),
        Err(OrchestrationError::WorkerCannotCoordinate)
    );
}

#[test]
fn a_worker_can_be_read_and_stopped_by_its_coordinator_only() {
    let setup = project();
    let task = task(&setup, "x");
    let worker = start(&setup, &task);
    *setup.host.output.lock().expect("lock") = "hello from the worker".to_owned();
    setup
        .host
        .set_status(&worker.session_id, AgentStatus::Working);

    let (_, reading) = setup
        .orchestration
        .read_worker("coordinator", &worker.id)
        .expect("read");
    assert_eq!(reading.output, "hello from the worker");
    assert!(reading.running);
    assert_eq!(reading.status, Some(AgentStatus::Working));
    assert_eq!(
        setup
            .orchestration
            .read_worker("someone else", &worker.id)
            .map(|_| ()),
        Err(OrchestrationError::NoRun),
        "another session has no run to read from"
    );

    setup
        .orchestration
        .stop_worker("coordinator", &worker.id)
        .expect("stop");
    assert!(!setup.host.running(&worker.session_id));
    assert_eq!(
        setup
            .orchestration
            .database
            .tasks_of_run(&setup.run.id)
            .expect("tasks")[0]
            .state,
        TaskState::Cancelled
    );
}

#[test]
fn a_stopped_worker_has_no_screen_left_to_read() {
    let setup = project();
    let task = task(&setup, "x");
    let worker = start(&setup, &task);
    setup
        .orchestration
        .stop_worker("coordinator", &worker.id)
        .expect("stop");

    let (_, reading) = setup
        .orchestration
        .read_worker("coordinator", &worker.id)
        .expect("read");
    assert_eq!(reading.output, "");
    assert!(!reading.running);
}

#[test]
fn a_worker_id_must_belong_to_the_coordinators_run() {
    let setup = project();
    assert_eq!(
        setup
            .orchestration
            .stop_worker("coordinator", "not-a-worker")
            .map(|_| ()),
        Err(OrchestrationError::UnknownWorker("not-a-worker".to_owned()))
    );
}

#[test]
fn a_task_names_only_tasks_of_its_run_and_needs_a_spec() {
    let setup = project();
    assert_eq!(
        setup
            .orchestration
            .create_task("coordinator", "x", vec!["missing".to_owned()])
            .map(|_| ()),
        Err(OrchestrationError::UnknownTask("missing".to_owned()))
    );
    assert!(matches!(
        setup
            .orchestration
            .create_task("coordinator", "   ", vec![]),
        Err(OrchestrationError::InvalidText(_))
    ));
}

#[test]
fn a_report_ends_the_task_and_the_coordinator_hears_it() {
    let setup = project();
    let one = task(&setup, "one");
    let first = start(&setup, &one);

    setup
        .orchestration
        .report(&first.session_id, Outcome::Done, "parser fixed")
        .expect("report");

    let check = setup
        .orchestration
        .check("coordinator", Duration::ZERO)
        .expect("check");
    assert_eq!(check.messages.len(), 1);
    assert_eq!(check.messages[0].kind, MessageKind::Done);
    assert_eq!(check.messages[0].body, "parser fixed");
    assert_eq!(
        check.messages[0].dispatch_id.as_deref(),
        Some(first.id.as_str())
    );
    assert_eq!(check.workers[0].task_state, TaskState::Done);
}

#[test]
fn a_finished_worker_is_closed_once_its_coordinator_has_heard_it() {
    let setup = project();
    let task = task(&setup, "x");
    let worker = start(&setup, &task);
    setup
        .orchestration
        .report(&worker.session_id, Outcome::Done, "ok")
        .expect("report");
    assert!(
        setup.host.running(&worker.session_id),
        "the report alone does not close the terminal"
    );

    let check = setup
        .orchestration
        .check("coordinator", Duration::ZERO)
        .expect("check");

    assert!(!setup.host.running(&worker.session_id));
    assert!(!check.workers[0].running);
    assert_eq!(
        check.messages[0].body, "ok",
        "the report stays with the coordinator"
    );
}

#[test]
fn a_finished_worker_stays_open_when_settings_keep_them() {
    let setup = project();
    setup.host.set_settings(OrchestrationConfig {
        release_finished_workers: false,
        ..enabled()
    });
    let task = task(&setup, "x");
    let worker = start(&setup, &task);
    setup
        .orchestration
        .report(&worker.session_id, Outcome::Failed, "no access")
        .expect("report");

    setup
        .orchestration
        .check("coordinator", Duration::ZERO)
        .expect("check");

    assert!(setup.host.running(&worker.session_id));
}

#[test]
fn a_report_is_refused_from_a_coordinator_or_after_the_first() {
    let setup = project();
    let task = task(&setup, "x");
    let worker = start(&setup, &task);
    assert_eq!(
        setup
            .orchestration
            .report("coordinator", Outcome::Done, "mine"),
        Err(OrchestrationError::NotAWorker)
    );
    setup
        .orchestration
        .report(&worker.session_id, Outcome::Failed, "no access")
        .expect("report");
    assert_eq!(
        setup
            .orchestration
            .report(&worker.session_id, Outcome::Done, "again"),
        Err(OrchestrationError::AlreadyReported("failed".to_owned()))
    );
}

#[test]
fn a_report_needs_a_summary() {
    let setup = project();
    let task = task(&setup, "x");
    let worker = start(&setup, &task);
    assert!(matches!(
        setup
            .orchestration
            .report(&worker.session_id, Outcome::Done, "   "),
        Err(OrchestrationError::InvalidText(_))
    ));
}

#[test]
fn a_check_hands_each_message_over_once() {
    let setup = project();
    let task = task(&setup, "x");
    let worker = start(&setup, &task);
    setup
        .orchestration
        .report(&worker.session_id, Outcome::Done, "ok")
        .expect("report");

    assert_eq!(
        setup
            .orchestration
            .check("coordinator", Duration::ZERO)
            .expect("check")
            .messages
            .len(),
        1
    );
    assert!(
        setup
            .orchestration
            .check("coordinator", Duration::ZERO)
            .expect("check")
            .messages
            .is_empty()
    );
}

#[test]
fn a_check_returns_as_soon_as_a_worker_reports() {
    let setup = project();
    let task = task(&setup, "x");
    let worker = start(&setup, &task);
    let started = Instant::now();

    let check = std::thread::scope(|scope| {
        scope.spawn(|| {
            std::thread::sleep(Duration::from_millis(300));
            setup
                .orchestration
                .report(&worker.session_id, Outcome::Done, "ok")
                .expect("report");
        });
        setup
            .orchestration
            .check("coordinator", Duration::from_secs(10))
    })
    .expect("check");

    assert_eq!(check.messages.len(), 1);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the check did not wait for the report"
    );
}

#[test]
fn a_worker_whose_session_ends_without_a_report_fails_its_task_and_is_said_once() {
    let setup = project();
    let task = task(&setup, "x");
    let worker = start(&setup, &task);
    setup.host.end(&worker.session_id);

    let check = setup
        .orchestration
        .check("coordinator", Duration::ZERO)
        .expect("check");
    assert_eq!(check.messages[0].kind, MessageKind::NoReport);
    assert_eq!(check.workers[0].task_state, TaskState::Failed);
    assert!(
        setup
            .orchestration
            .check("coordinator", Duration::ZERO)
            .expect("check")
            .messages
            .is_empty()
    );
}

#[test]
fn an_agent_that_cannot_report_is_noticed_when_its_turn_ends_once() {
    let setup = project();
    let task = task(&setup, "x");
    let worker = setup
        .orchestration
        .start_worker(
            "coordinator",
            &task.id,
            HarnessId::Omp,
            WorkerWorktree::New,
            None,
        )
        .expect("start");
    setup.host.set_status(&worker.session_id, AgentStatus::Done);

    let check = setup
        .orchestration
        .check("coordinator", Duration::ZERO)
        .expect("check");
    assert_eq!(check.messages[0].kind, MessageKind::NoReport);
    assert!(
        setup
            .orchestration
            .check("coordinator", Duration::ZERO)
            .expect("check")
            .messages
            .is_empty()
    );
}

#[test]
fn an_agent_without_the_report_tool_is_not_told_to_report() {
    let setup = project();
    let task = task(&setup, "plain");
    setup
        .orchestration
        .start_worker(
            "coordinator",
            &task.id,
            HarnessId::Omp,
            WorkerWorktree::New,
            None,
        )
        .expect("start");
    assert_eq!(
        setup.host.prompts.lock().expect("lock").as_slice(),
        ["plain"]
    );
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

#[test]
fn a_worker_waiting_for_an_answer_is_said_once_for_each_wait() {
    let setup = project();
    let task = task(&setup, "x");
    let worker = start(&setup, &task);
    let waiting = |setup: &Setup| {
        setup
            .orchestration
            .check("coordinator", Duration::ZERO)
            .expect("check")
            .messages
            .into_iter()
            .map(|message| message.kind)
            .collect::<Vec<_>>()
    };

    setup
        .host
        .set_status(&worker.session_id, AgentStatus::NeedsYou);
    assert_eq!(waiting(&setup), [MessageKind::NeedsYou]);
    assert!(
        waiting(&setup).is_empty(),
        "the same wait is not said again"
    );

    setup
        .host
        .set_status(&worker.session_id, AgentStatus::Working);
    assert!(waiting(&setup).is_empty());
    setup
        .host
        .set_status(&worker.session_id, AgentStatus::NeedsYou);
    assert_eq!(
        waiting(&setup),
        [MessageKind::NeedsYou],
        "a new wait is said again"
    );
    assert_eq!(
        setup.host.stopped.lock().expect("lock").len(),
        0,
        "a worker that waits is not closed"
    );
}

/// A bot as coordinator, with a run in `/repo/app`, and a host that refuses to start workers with
/// `refuse` as the reason, when it is given.
fn bot_coordinator(refuse: Option<&str>) -> (Orchestration, Arc<FakeHost>, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let database = Arc::new(AppDatabase::open(&dir.path().join("app.db")).expect("database"));
    let host = FakeHost::with_project("bot:ada");
    *host.refuse.lock().expect("lock") = refuse.map(str::to_owned);
    let host = Arc::new(host);
    let orchestration = Orchestration::new(database, Arc::clone(&host) as Arc<dyn WorkerHost>);
    orchestration.start_run("bot:ada", "Audit").expect("run");
    (orchestration, host, dir)
}

#[test]
fn a_worker_a_bot_starts_never_stops_to_ask_for_permission_but_an_agents_may() {
    let (orchestration, host, _dir) = bot_coordinator(None);
    let task = orchestration
        .create_task("bot:ada", "Audite", vec![])
        .expect("task");
    orchestration
        .start_worker(
            "bot:ada",
            &task.id,
            HarnessId::Claude,
            WorkerWorktree::New,
            None,
        )
        .expect("started");
    assert!(
        host.started_commands()[0].1.starts_with("claude --yolo"),
        "{:?}",
        host.started_commands()
    );

    let agent = project();
    start(&agent, &task_of(&agent));
    assert!(
        !agent.host.started_commands()[0].1.contains("--yolo"),
        "an agent coordinator keeps the user's launch mode"
    );
}

fn task_of(setup: &Setup) -> Task {
    task(setup, "Audite")
}

#[test]
fn a_bot_that_may_not_start_workers_here_is_told_how_to_ask_and_nothing_starts() {
    let (orchestration, host, _dir) = bot_coordinator(Some("peça com bot_request"));
    let task = orchestration
        .create_task("bot:ada", "Audite", vec![])
        .expect("task");

    let refused = orchestration.start_worker(
        "bot:ada",
        &task.id,
        HarnessId::Claude,
        WorkerWorktree::New,
        None,
    );

    assert_eq!(
        refused.map(|_| ()),
        Err(OrchestrationError::Host("peça com bot_request".to_owned()))
    );
    assert!(host.started_commands().is_empty(), "no terminal was opened");
    let tasks = orchestration.workers_of_bot("ada").expect("workers");
    assert!(tasks.is_empty());
}

#[test]
fn a_coordinator_answers_a_worker_that_waits_in_one_clean_line() {
    let setup = project();
    let worker = start(&setup, &task(&setup, "Audite"));

    setup
        .orchestration
        .reply_worker("coordinator", &worker.id, "  sim, pode seguir  ")
        .expect("answered");
    assert_eq!(
        *setup.host.sent.lock().expect("lock"),
        [(worker.session_id.clone(), "sim, pode seguir".to_owned())]
    );

    for bad in [
        "",
        "   ",
        "linha um\nlinha dois",
        "com \u{1b}[31m escape",
        &"x".repeat(2001),
    ] {
        assert!(
            matches!(
                setup
                    .orchestration
                    .reply_worker("coordinator", &worker.id, bad),
                Err(OrchestrationError::InvalidText(_))
            ),
            "{bad:?}"
        );
    }
    assert_eq!(
        setup.host.sent.lock().expect("lock").len(),
        1,
        "only the clean answer went"
    );
}

#[test]
fn only_the_workers_of_the_coordinators_own_run_can_be_answered_and_only_while_they_run() {
    let setup = project();
    let worker = start(&setup, &task(&setup, "Audite"));

    assert!(matches!(
        setup
            .orchestration
            .reply_worker("coordinator", "no-such-worker", "sim"),
        Err(OrchestrationError::UnknownWorker(_))
    ));
    setup.host.end(&worker.session_id);
    assert!(matches!(
        setup
            .orchestration
            .reply_worker("coordinator", &worker.id, "sim"),
        Err(OrchestrationError::Host(_))
    ));
}
