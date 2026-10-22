//! One real run: Claude Code with Haiku as the coordinator and as its two workers, against the app's
//! own terminals, hub, worktrees and orchestration. No window opens, but it spends tokens and needs
//! `claude` on the PATH, so it runs only on request:
//!
//!     cargo build -p lynx-ide
//!     LYNX_LIVE=1 cargo test -p lynx-ide a_run_with_haiku -- --ignored --nocapture
//!
//! `LYNX_LIVE_MODE=manual` starts the agents without their YOLO flag. The folder the run used stays
//! on disk, and its path is printed.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::agents::activity::AgentStatus;
use crate::agents::agent_prompt_file;
use crate::agents::harness::HarnessId;
use crate::agents::hook_hub::{HookHub, ToolHandler};
use crate::agents::launch_line::launch_line;
use crate::browser::{PageOwner, SessionOwners};
use crate::config::app_config::{LaunchMode, OrchestrationConfig};
use crate::git::test_repository::{commit_file_in, set_identity};
use crate::orchestration::host::visible_text;
use crate::orchestration::model::TaskState;
use crate::orchestration::router::ToolRouter;
use crate::orchestration::service::{Orchestration, WorkerHost, quote_for_shell};
use crate::persistence::AppDatabase;
use crate::terminal::Shell;
use crate::terminal::terminal_events::TerminalEventSink;
use crate::terminal::terminal_manager::TerminalManager;
use crate::workspace::workspace_service::WorkspaceService;

const MODEL: &str = "claude-haiku-5-5";

/// The app's side of the worker host, as `AppWorkers` has it, without a window.
struct LiveHost {
    owners: SessionOwners,
    terminals: TerminalManager,
    workspace: Arc<WorkspaceService>,
    launch: BTreeMap<HarnessId, String>,
    mode: LaunchMode,
}

impl WorkerHost for LiveHost {
    fn settings(&self) -> OrchestrationConfig {
        OrchestrationConfig {
            enabled: true,
            ..OrchestrationConfig::default()
        }
    }

    fn worktree_of(&self, session: &str) -> Option<String> {
        match self.owners.get(session)? {
            PageOwner::Workspace { worktree_path } => Some(worktree_path),
            PageOwner::Bot { .. } => None,
        }
    }

    fn new_worktree(&self, from: &str, name: &str) -> Result<String, String> {
        let repo = self
            .workspace
            .repo_id_of_worktree(from)
            .ok_or_else(|| "Lynx does not know the project".to_owned())?;
        self.workspace
            .create_worktree(&repo, name, None)
            .map(|worktree| worktree.path)
            .map_err(|error| error.to_string())
    }

    fn prompt_file(&self, text: &str) -> Result<String, String> {
        agent_prompt_file::write(text)
            .map(|path| path.to_string_lossy().into_owned())
            .map_err(|error| error.to_string())
    }

    fn launch_line(&self, harness: HarnessId) -> String {
        launch_line(harness, self.mode, &self.launch)
    }

    fn start(&self, worktree: &str, command: &str) -> Result<String, String> {
        self.terminals
            .create(worktree, Some(command), 120, 32)
            .map(|info| info.id)
            .map_err(|error| error.to_string())
    }

    fn running(&self, session: &str) -> bool {
        self.terminals.info(session).is_ok_and(|info| info.running)
    }

    fn status(&self, session: &str) -> Option<AgentStatus> {
        self.terminals
            .info(session)
            .ok()
            .and_then(|info| info.agent_status)
    }

    fn output(&self, session: &str) -> Result<String, String> {
        self.terminals
            .snapshot(session)
            .map(|snapshot| visible_text(&snapshot.data))
            .map_err(|error| error.to_string())
    }

    fn stop(&self, session: &str) {
        let _ = self.terminals.close(session);
    }
}

/// The browser is not part of this run; its tools answer that they are not.
struct NoBrowser;

impl ToolHandler for NoBrowser {
    fn call(&self, _session: &str, _agent: &str, tool: &str, _arguments: Value) -> Value {
        json!({ "content": [{ "type": "text", "text": format!("{tool} is not part of this run") }], "isError": true })
    }
}

/// Keeps everything each terminal printed, so a worker's screen survives its release.
#[derive(Default)]
struct RecordingSink {
    printed: Mutex<HashMap<String, String>>,
}

impl RecordingSink {
    fn text(&self, id: &str) -> String {
        let printed = self.printed.lock().unwrap_or_else(PoisonError::into_inner);
        printed.get(id).cloned().unwrap_or_default()
    }
}

impl TerminalEventSink for RecordingSink {
    fn output(&self, id: &str, _seq: u32, data: String) {
        self.printed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(id.to_owned())
            .or_default()
            .push_str(&data);
    }
    fn exit(&self, _id: &str, _exit_code: Option<i32>) {}
}

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .status()
        .expect("git runs");
    assert!(status.success(), "git {args:?} failed");
}

/// Marks the run's repository as trusted in Claude's settings while the run lasts, as a person does
/// once when they first open a project, and takes the mark away when the test ends. Worktrees of
/// the repository inherit that trust.
struct TrustedRepository {
    config: PathBuf,
    key: String,
}

impl TrustedRepository {
    fn mark(repo: &Path) -> Self {
        let trusted = Self {
            config: settings_file(),
            key: fs::canonicalize(repo)
                .expect("the repository")
                .to_string_lossy()
                .into_owned(),
        };
        let key = trusted.key.clone();
        edit_projects(&trusted.config, |projects| {
            projects.insert(key, json!({ "hasTrustDialogAccepted": true }));
        })
        .expect("mark the repository as trusted");
        trusted
    }
}

impl Drop for TrustedRepository {
    fn drop(&mut self) {
        let key = self.key.clone();
        let _ = edit_projects(&self.config, |projects| {
            projects.remove(&key);
        });
    }
}

/// Claude's settings file: `CLAUDE_CONFIG_DIR` when set, else the home folder.
fn settings_file() -> PathBuf {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .map_or_else(|| std::env::home_dir().expect("home folder"), PathBuf::from)
        .join(".claude.json")
}

/// Changes the projects in Claude's settings file, writing through a copy so a crash keeps the old
/// file, and keeping the file's permissions.
fn edit_projects(
    config: &Path,
    edit: impl FnOnce(&mut serde_json::Map<String, Value>),
) -> std::io::Result<()> {
    let mut root: Value =
        serde_json::from_str(&fs::read_to_string(config)?).map_err(std::io::Error::other)?;
    if let Some(projects) = root.get_mut("projects").and_then(Value::as_object_mut) {
        edit(projects);
    }
    let mut text = serde_json::to_string_pretty(&root).map_err(std::io::Error::other)?;
    text.push('\n');
    let copy = config.with_extension("json.test-tmp");
    fs::write(&copy, text)?;
    fs::set_permissions(&copy, fs::metadata(config)?.permissions())?;
    fs::rename(&copy, config)
}

fn coordinator_prompt() -> String {
    "You coordinate a run in this project. The user already answered the questions you would ask: \
     every worker uses the agent claude with the model claude-haiku-5-5, and each worker gets a new \
     worktree of its own.\n\n\
     Do this, in order:\n\
     1. Call lynx_run_start with the title \"Two files\".\n\
     2. Call lynx_task_create twice. Spec one: \"Create the file alpha.txt in the worktree root with \
     exactly one line: alpha. Then call lynx_report with outcome done and a one-line summary.\" Spec \
     two: \"Create the file beta.txt in the worktree root with exactly one line: beta. Then call \
     lynx_report with outcome done and a one-line summary.\"\n\
     3. For each task, call lynx_worker_start with its taskId, agent \"claude\", worktree \"new\", \
     and model \"claude-haiku-5-5\".\n\
     4. Call lynx_check until both workers have reported.\n\
     5. Reply with one line per worker: its workerId and its summary. Then stop."
        .to_owned()
}

#[test]
#[ignore = "spends tokens: needs LYNX_LIVE=1, a built lynx-ide and claude on the PATH"]
fn a_run_with_haiku_finishes_in_worktrees_of_its_own() {
    if std::env::var_os("LYNX_LIVE").is_none() {
        eprintln!("skipped: set LYNX_LIVE=1 to spend tokens on a real run");
        return;
    }
    let binary = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/debug/lynx-ide");
    assert!(
        binary.exists(),
        "build the app first: cargo build -p lynx-ide"
    );
    let mode = if std::env::var("LYNX_LIVE_MODE").is_ok_and(|value| value == "manual") {
        LaunchMode::Manual
    } else {
        LaunchMode::Yolo
    };

    let root = std::env::temp_dir().join(format!("lynx-live-{}", uuid::Uuid::new_v4()));
    let repo = root.join("app");
    fs::create_dir_all(&repo).expect("repository folder");
    git(&repo, &["init", "-q", "-b", "main"]);
    set_identity(&repo);
    commit_file_in(&repo, "README.md", "# app\n", "init");
    eprintln!("run folder: {}", root.display());

    let database = Arc::new(AppDatabase::open(&root.join("app.db")).expect("database"));
    let workspace = Arc::new(WorkspaceService::new(
        AppDatabase::open(&root.join("app.db")).expect("database"),
        Arc::new({
            let worktrees = root.join("worktrees");
            move || Some(worktrees.clone())
        }),
    ));
    let project = workspace
        .add_repo(&repo.to_string_lossy())
        .expect("add the repository");
    // The sidebar reads a project's worktrees when it opens; the orchestration finds a coordinator's
    // project from that list, so the test reads it the same way.
    workspace
        .list_worktrees(&project.id)
        .expect("list the worktrees");

    let owners = SessionOwners::default();
    let hub = HookHub::start();
    hub.publish_address(&root.join("hub.json"))
        .expect("publish the hub address");
    let mcp = root.join("mcp.json");
    fs::write(
        &mcp,
        json!({ "mcpServers": { "lynx": { "command": binary.to_string_lossy(), "args": ["--mcp", "claude"] } } })
            .to_string(),
    )
    .expect("write the MCP entry");
    let launch = BTreeMap::from([(
        HarnessId::Claude,
        format!("--mcp-config {}", mcp.to_string_lossy()),
    )]);

    let sink = Arc::new(RecordingSink::default());
    let terminals = TerminalManager::new(
        Arc::new(|| Shell::from_config(None)),
        Arc::clone(&sink) as Arc<dyn TerminalEventSink>,
        hub.clone(),
        owners.clone(),
        None,
        Some(Arc::clone(&database)),
    );
    let orchestration = Arc::new(Orchestration::new(
        Arc::clone(&database),
        Arc::new(LiveHost {
            owners: owners.clone(),
            terminals: terminals.clone(),
            workspace: Arc::clone(&workspace),
            launch: launch.clone(),
            mode,
        }),
    ));
    hub.set_tool_handler(Arc::new(ToolRouter::new(
        Arc::new(NoBrowser),
        Arc::clone(&orchestration),
    )));

    // The coordinator runs in the repository, which a person has trusted once before using Claude in it.
    let _trusted = TrustedRepository::mark(&repo);
    let prompt_file = agent_prompt_file::write(&coordinator_prompt()).expect("prompt file");
    let command = format!(
        "{} --model {MODEL} {}",
        launch_line(HarnessId::Claude, mode, &launch),
        quote_for_shell(
            &format!(
                "Read {} and follow the instructions in it.",
                prompt_file.display()
            ),
            false
        )
    );
    let coordinator = terminals
        .create(&repo.to_string_lossy(), Some(&command), 120, 32)
        .expect("start the coordinator");
    eprintln!("coordinator: {}", coordinator.id);

    let timeout = std::env::var("LYNX_LIVE_TIMEOUT_SECS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(600);
    let deadline = Instant::now() + Duration::from_secs(timeout);
    let run = loop {
        if let Some(run) = orchestration
            .database
            .latest_run_of(&coordinator.id)
            .expect("runs")
        {
            let tasks = orchestration.database.tasks_of_run(&run.id).expect("tasks");
            if tasks.len() == 2
                && tasks
                    .iter()
                    .all(|task| matches!(task.state, TaskState::Done | TaskState::Failed))
            {
                break Some(run);
            }
        }
        if Instant::now() >= deadline {
            break None;
        }
        std::thread::sleep(Duration::from_secs(5));
    };

    let coordinator_text = visible_text(&sink.text(&coordinator.id));
    fs::write(root.join("coordinator.txt"), &coordinator_text).expect("save coordinator screen");
    let mut workers: Vec<(_, String, Vec<String>)> = Vec::new();
    if let Some(run) = &run {
        for dispatch in orchestration
            .database
            .dispatches_of_run(&run.id)
            .expect("workers")
        {
            let screen = visible_text(&sink.text(&dispatch.session_id));
            fs::write(
                root.join(format!("worker-{}.txt", dispatch.task_id)),
                &screen,
            )
            .expect("save worker screen");
            let files: Vec<String> = ["alpha.txt", "beta.txt"]
                .iter()
                .filter_map(|name| {
                    let text =
                        fs::read_to_string(Path::new(&dispatch.worktree_path).join(name)).ok()?;
                    Some(format!("{name}: {}", text.trim()))
                })
                .collect();
            workers.push((dispatch, screen, files));
        }
    }

    eprintln!(
        "---- coordinator (tail) ----\n{}",
        tail(&coordinator_text, 1500)
    );
    for (dispatch, screen, files) in &workers {
        eprintln!(
            "---- worker {} in {} wrote {files:?} (tail) ----\n{}",
            dispatch.task_id,
            dispatch.worktree_path,
            tail(screen, 800)
        );
    }
    terminals.close_all();

    let run = run.unwrap_or_else(|| panic!("the run did not finish within {timeout} seconds"));
    let tasks = orchestration.database.tasks_of_run(&run.id).expect("tasks");
    assert!(
        tasks.iter().all(|task| task.state == TaskState::Done),
        "both tasks are done: {:?}",
        tasks.iter().map(|task| task.state).collect::<Vec<_>>()
    );
    assert_eq!(workers.len(), 2, "two workers started");
    for (dispatch, screen, files) in &workers {
        assert!(
            Path::new(&dispatch.worktree_path).starts_with(root.join("worktrees")),
            "{} is a worktree of its own",
            dispatch.worktree_path
        );
        assert_eq!(files.len(), 1, "each worker wrote its file: {files:?}");
        // The screen draws words with cursor moves, so the spaces are compared away.
        let compact: String = screen.chars().filter(|c| !c.is_whitespace()).collect();
        assert!(
            !compact.contains("Quicksafetycheck"),
            "the worker is not held by Claude's folder question"
        );
    }
    eprintln!("run folder kept: {}", root.display());
}

fn tail(text: &str, chars: usize) -> String {
    let count = text.chars().count();
    text.chars().skip(count.saturating_sub(chars)).collect()
}
