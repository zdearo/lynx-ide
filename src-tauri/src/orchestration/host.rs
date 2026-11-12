//! The app's side of the orchestration: its terminals, worktrees, prompt files and settings, read
//! from the managed services when a call needs them.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Manager};
use tauri_specta::Event;

use crate::agents::activity::AgentStatus;
use crate::agents::agent_prompt_file;
use crate::agents::harness::HarnessId;
use crate::agents::hook_install::HookInstallState;
use crate::agents::hook_targets::HookTargets;
use crate::agents::launch_line::launch_line;
use crate::bots::bot_bridge::{bot_of_session, parse_session};
use crate::bots::bot_chat::BotChat;
use crate::config::ConfigStore;
use crate::config::app_config::{LaunchMode, OrchestrationConfig};
use crate::git::git_command::git;
use crate::team::team_events::TeamChanged;
use crate::terminal::terminal_manager::TerminalManager;
use crate::workspace::workspace_events::WorktreeCreated;
use crate::workspace::workspace_service::WorkspaceService;

use super::agent_wake::AgentWakes;
use super::service::WorkerHost;

/// What a bot is told when one of its workers changed: it looks at its team and tells the person.
pub(super) const WAKE_NOTE: &str = "Algo mudou nas suas tarefas: um worker terminou, precisa de uma resposta ou parou. Chame lynx_check com waitSeconds 0 para ver o que aconteceu e relate à pessoa.";

/// How often, and how many times, a worker's screen is looked at for its prompt before giving up (30 s).
const SUBMIT_POLL: Duration = Duration::from_millis(500);
const SUBMIT_TRIES: u32 = 60;

/// Workers are terminals of the app, in worktrees of the app's projects.
pub struct AppWorkers {
    app: AppHandle,
    config: Arc<ConfigStore>,
}

impl AppWorkers {
    pub fn new(app: AppHandle, config: Arc<ConfigStore>) -> Self {
        Self { app, config }
    }

    fn terminals(&self) -> tauri::State<'_, TerminalManager> {
        self.app.state::<TerminalManager>()
    }
}

impl WorkerHost for AppWorkers {
    fn settings(&self) -> OrchestrationConfig {
        self.config.config().orchestration
    }

    fn notify(&self, session: &str) {
        let Some((bot, label)) = parse_session(session) else {
            // An agent orchestrator is woken through its terminal, once it is idle.
            if let Some(wakes) = self.app.try_state::<Arc<AgentWakes>>() {
                wakes.note(session);
            }
            return;
        };
        let chat = Arc::clone(&*self.app.state::<Arc<BotChat>>());
        let (bot, label) = (bot.to_owned(), label.to_owned());
        chat.wake(&bot, &label, WAKE_NOTE);
        BotChat::kick(chat, bot, label);
    }

    fn checked(&self, coordinator: &str) {
        if let Some(wakes) = self.app.try_state::<Arc<AgentWakes>>() {
            wakes.cancel(coordinator);
        }
    }

    fn changed(&self) {
        let _ = TeamChanged {}.emit(&self.app);
    }

    fn submit_when_shown(&self, session: &str, text: &str) {
        let app = self.app.clone();
        let session = session.to_owned();
        let text = text.to_owned();
        std::thread::spawn(move || {
            for _ in 0..SUBMIT_TRIES {
                std::thread::sleep(SUBMIT_POLL);
                let terminals = app.state::<TerminalManager>();
                let Ok(snapshot) = terminals.snapshot(&session) else {
                    return;
                };
                if drawn_twice(&visible_text(&snapshot.data), &text) {
                    let _ = terminals.write(&session, "\r");
                    return;
                }
            }
        });
    }

    fn new_worktree(&self, from: &str, name: &str) -> Result<String, String> {
        let workspace = self.app.state::<WorkspaceService>();
        let repo = workspace
            .repo_id_of_worktree(from)
            .ok_or_else(|| "Lynx does not know the project this agent works in".to_owned())?;
        let worktree = workspace
            .create_worktree(&repo, name, None)
            .map_err(|error| error.to_string())?;
        // The screen reads a project's worktrees when told to, so one made here is missing until then.
        let told = WorktreeCreated {
            repo_id: repo.clone(),
        }
        .emit(&self.app);
        if let Err(error) = told {
            eprintln!(
                "lynx: could not tell the screen that {repo} has worktree {}: {error}",
                worktree.path
            );
        }
        Ok(worktree.path)
    }

    fn prompt_file(&self, text: &str) -> Result<String, String> {
        agent_prompt_file::write(text)
            .map(|path| path.to_string_lossy().into_owned())
            .map_err(|error| error.to_string())
    }

    fn launch_line(&self, harness: HarnessId, unattended: bool) -> String {
        let config = self.config.config();
        let mode = if unattended {
            LaunchMode::Yolo
        } else {
            config.agents.launch_mode
        };
        launch_line(harness, mode, &config.agents.launch_arguments)
    }

    fn agent_on(&self, harness: HarnessId) -> bool {
        self.app.state::<HookTargets>().status(harness).state == HookInstallState::Installed
    }

    fn allow_start(&self, coordinator: &str, worktree: &str) -> Result<(), String> {
        let Some(bot) = bot_of_session(coordinator) else {
            return Ok(());
        };
        let repo = self
            .app
            .state::<WorkspaceService>()
            .repo_id_of_worktree(worktree);
        let allowed = self
            .app
            .state::<Arc<BotChat>>()
            .worker_grants(bot)
            .is_ok_and(|grants| {
                repo.is_some_and(|repo| grants.iter().any(|grant| grant.id == repo))
            });
        if allowed {
            Ok(())
        } else {
            Err("Você ainda não pode lançar agentes neste projeto. Peça à pessoa com bot_request, kind \"workers\" e o projeto em items; diga no motivo o que os agentes vão fazer, porque eles rodam sem pedir permissão a ninguém.".to_owned())
        }
    }

    fn send(&self, session: &str, text: &str) -> Result<(), String> {
        super::agent_wake::type_and_submit(&self.terminals(), session, text)
            .map_err(|error| error.to_string())
    }

    fn land(&self, from: &str, into: &str) -> Result<String, String> {
        land_branch(Path::new(from), Path::new(into))
    }

    fn start(&self, worktree: &str, command: &str) -> Result<String, String> {
        self.terminals()
            .create(worktree, Some(command), 120, 32)
            .map(|info| info.id)
            .map_err(|error| error.to_string())
    }

    fn running(&self, session: &str) -> bool {
        self.terminals()
            .info(session)
            .is_ok_and(|info| info.running)
    }

    fn status(&self, session: &str) -> Option<AgentStatus> {
        self.terminals()
            .info(session)
            .ok()
            .and_then(|info| info.agent_status)
    }

    fn output(&self, session: &str) -> Result<String, String> {
        self.terminals()
            .snapshot(session)
            .map(|snapshot| visible_text(&snapshot.data))
            .map_err(|error| error.to_string())
    }

    fn stop(&self, session: &str) {
        // Closed, not only ended: the session leaves the list and the screen drops its tab.
        let _ = self.terminals().close_and_announce(session);
    }
}

/// Merges the branch checked out in `from` into the one in `into`. Both worktrees must be clean, so no work is lost
/// or mixed in, and a merge that stops is aborted so `into` is left as it was.
fn land_branch(from: &Path, into: &Path) -> Result<String, String> {
    let ensure_clean = |path: &Path, who: &str| -> Result<(), String> {
        let status = git(path)
            .args(["status", "--porcelain"])
            .run()
            .map_err(|error| error.to_string())?;
        if status.trim().is_empty() {
            Ok(())
        } else {
            Err(format!(
                "{who} has uncommitted changes: they must be committed before landing"
            ))
        }
    };
    ensure_clean(from, "the worker's worktree")?;
    ensure_clean(into, "the team's worktree")?;
    let branch = git(from)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .run()
        .map_err(|error| error.to_string())?;
    let branch = branch.trim();
    if branch == "HEAD" {
        return Err(
            "the worker's worktree is not on a branch, so there is nothing to merge".to_owned(),
        );
    }
    match git(into)
        .args(["merge", "--no-ff", "--no-edit", branch])
        .run()
    {
        Ok(_) => Ok(format!("merged {branch}")),
        Err(error) => {
            let _ = git(into).args(["merge", "--abort"]).run();
            Err(format!(
                "merging {branch} did not go cleanly and was undone: {error}"
            ))
        }
    }
}

/// The text a screen shows, without the escape sequences that move the cursor or colour it.
pub fn visible_text(data: &str) -> String {
    let mut out = String::with_capacity(data.len());
    let mut chars = data.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            if c == '\r' || c == '\n' || c == '\t' || !c.is_control() {
                out.push(c);
            }
            continue;
        }
        match chars.next() {
            // Control sequence: parameters, then one final byte between @ and ~.
            Some('[') => {
                for next in chars.by_ref() {
                    if ('\u{40}'..='\u{7e}').contains(&next) {
                        break;
                    }
                }
            }
            // Operating system command: ends at BEL or at ESC \.
            Some(']') => {
                while let Some(next) = chars.next() {
                    if next == '\u{7}' {
                        break;
                    }
                    if next == '\u{1b}' {
                        chars.next();
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// Whether `text` is on screen twice. The shell echoes the command that starts an agent, and the agent
/// draws the same text in its input once it is ready. Spaces are ignored, since a screen wraps lines.
pub fn drawn_twice(screen: &str, text: &str) -> bool {
    let squeezed = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    let wanted = squeezed(text);
    !wanted.is_empty() && squeezed(screen).matches(&wanted).count() >= 2
}

#[cfg(test)]
mod tests {
    use super::{drawn_twice, land_branch, visible_text};
    use crate::git::test_repository::{TestRepository, commit_file_in};

    #[test]
    fn escape_sequences_are_dropped_and_the_text_kept() {
        assert_eq!(visible_text("\u{1b}[31mred\u{1b}[0m text"), "red text");
        assert_eq!(visible_text("\u{1b}]0;title\u{7}line\r\n"), "line\r\n");
        assert_eq!(visible_text("a\u{7}b"), "ab", "a bell is not text");
    }

    #[test]
    fn a_prompt_counts_once_the_agent_draws_it_too() {
        let text = "Read /w/p.md and follow the instructions in it.";
        let echoed = "$ opencode --prompt 'Read /w/p.md and follow the instructions in it.'\n";
        assert!(
            !drawn_twice(echoed, text),
            "the echoed command alone is not enough"
        );

        let drawn = format!("{echoed}Read /w/p.md and follow the\ninstructions in it.");
        assert!(
            drawn_twice(&drawn, text),
            "a line break in the input does not hide it"
        );
    }

    #[test]
    fn landing_merges_a_clean_branch_and_undoes_one_that_conflicts() {
        let repo = TestRepository::new();
        let side = repo.path.with_file_name("side");
        repo.git(&[
            "worktree",
            "add",
            "-q",
            "-b",
            "task",
            &side.to_string_lossy(),
        ]);

        commit_file_in(&side, "new.txt", "from the worker\n", "worker work");
        std::fs::write(side.join("dirty.txt"), "x").expect("write");
        assert!(
            land_branch(&side, &repo.path)
                .expect_err("uncommitted work blocks landing")
                .contains("uncommitted"),
        );
        std::fs::remove_file(side.join("dirty.txt")).expect("remove");

        let said = land_branch(&side, &repo.path).expect("a clean branch lands");
        assert_eq!(said, "merged task");
        assert!(repo.path.join("new.txt").exists());

        commit_file_in(&side, "README.md", "worker\n", "worker edit");
        commit_file_in(&repo.path, "README.md", "lead\n", "lead edit");
        let error = land_branch(&side, &repo.path).expect_err("a conflict is refused");
        assert!(error.contains("undone"), "{error}");
        assert!(
            repo.git(&["status", "--porcelain"]).trim().is_empty(),
            "the team's worktree is left as it was"
        );
    }
}
