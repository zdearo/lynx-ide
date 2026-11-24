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
use crate::agents::launch_line::launch_line_with_effort;
use crate::bots::bot_bridge::{bot_of_session, parse_session};
use crate::bots::bot_chat::BotChat;
use crate::config::ConfigStore;
use crate::config::app_config::{AgentEffort, LaunchMode, OrchestrationConfig};
use crate::git::git_command::git;
use crate::team::team_events::TeamChanged;
use crate::terminal::Shell;
use crate::terminal::pasted_text::as_paste;
use crate::terminal::terminal_manager::TerminalManager;
use crate::workspace::workspace_events::WorktreeCreated;
use crate::workspace::workspace_service::WorkspaceService;

use super::agent_wake::{AgentWakes, free_to_type, type_and_submit};
use super::service::{FirstPrompt, WorkerHost, quote_for_shell};

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
        chat.wake_reports(&bot, &label);
        BotChat::kick(chat, bot, label);
    }

    fn changed(&self) {
        let _ = TeamChanged {}.emit(&self.app);
    }

    fn submit_when_shown(&self, session: &str, text: &str, echoed: bool) {
        let app = self.app.clone();
        let session = session.to_owned();
        let text = text.to_owned();
        // The typed start line counts once when it carries the text, so the agent's own draw is the second sight.
        let times = if echoed { 2 } else { 1 };
        std::thread::spawn(move || {
            for _ in 0..SUBMIT_TRIES {
                std::thread::sleep(SUBMIT_POLL);
                let terminals = app.state::<TerminalManager>();
                let Ok(snapshot) = terminals.snapshot(&session) else {
                    return;
                };
                if drawn_at_least(&visible_text(&snapshot.data), &text, times) {
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

    fn first_prompt(&self, path: &str, text: &str) -> FirstPrompt {
        let shell = Shell::from_config(self.config.config().terminal.shell.as_deref());
        first_prompt_for(&shell.program, path, text, cfg!(windows))
    }

    fn launch_line(
        &self,
        harness: HarnessId,
        unattended: bool,
        effort: Option<AgentEffort>,
    ) -> String {
        let config = self.config.config();
        let mode = if unattended {
            LaunchMode::Yolo
        } else {
            config.agents.launch_mode
        };
        launch_line_with_effort(harness, mode, &config.agents.launch_arguments, effort)
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
        type_and_submit(&self.terminals(), session, text).map_err(|error| error.to_string())
    }

    fn ready_for_input(&self, session: &str) -> bool {
        let draft = self
            .app
            .try_state::<Arc<AgentWakes>>()
            .is_some_and(|wakes| wakes.has_draft(session));
        free_to_type(self.status(session), draft)
    }

    fn send_pasted(&self, session: &str, text: &str) -> Result<(), String> {
        type_and_submit(&self.terminals(), session, &as_paste(text))
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

/// Characters of a prompt that must be on screen for it to count as drawn. A short start of it survives a screen that
/// wraps or boxes the input, where the whole text would not match.
const PROBE_CHARS: usize = 200;

/// Whether the start of `text` is on screen at least `times` times. Spaces are ignored, since a screen wraps lines.
pub fn drawn_at_least(screen: &str, text: &str, times: usize) -> bool {
    let squeezed = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    let probe: String = squeezed(text).chars().take(PROBE_CHARS).collect();
    !probe.is_empty() && squeezed(screen).matches(&probe).count() >= times
}

/// Shells whose command line reads `$(cat file)`, so the file's text is the agent's first message.
const POSIX_SHELLS: &[&str] = &["sh", "bash", "zsh", "dash", "ksh", "fish"];
/// PowerShell reads `$(Get-Content -Raw file)` the same way.
const POWERSHELLS: &[&str] = &["pwsh", "powershell"];

/// How an agent gets its first prompt, from the shell that types its start line. The shell reads the prompt file
/// into the agent's first message when it can. Otherwise, as cmd.exe must, the agent is told to read the file.
/// `shown` is what the agent draws in its input, and `echoed` says the typed line carries it too.
pub fn first_prompt_for(program: &str, path: &str, text: &str, windows: bool) -> FirstPrompt {
    let base = program.rsplit(['/', '\\']).next().unwrap_or(program);
    let name = base
        .strip_suffix(".exe")
        .unwrap_or(base)
        .to_ascii_lowercase();
    if !windows && POSIX_SHELLS.contains(&name.as_str()) {
        return FirstPrompt {
            argument: format!("\"$(cat {})\"", quote_for_shell(path, false)),
            shown: text.to_owned(),
            echoed: false,
        };
    }
    if windows && POWERSHELLS.contains(&name.as_str()) {
        return FirstPrompt {
            argument: format!("\"$(Get-Content -Raw '{}')\"", path.replace('\'', "''")),
            shown: text.to_owned(),
            echoed: false,
        };
    }
    let instruction = format!("Read {path} and follow the instructions in it.");
    FirstPrompt {
        argument: quote_for_shell(&instruction, windows),
        shown: instruction,
        echoed: true,
    }
}

#[cfg(test)]
mod tests {
    use super::{drawn_at_least, first_prompt_for, land_branch, visible_text};
    use crate::git::test_repository::{TestRepository, commit_file_in};

    #[test]
    fn escape_sequences_are_dropped_and_the_text_kept() {
        assert_eq!(visible_text("\u{1b}[31mred\u{1b}[0m text"), "red text");
        assert_eq!(visible_text("\u{1b}]0;title\u{7}line\r\n"), "line\r\n");
        assert_eq!(visible_text("a\u{7}b"), "ab", "a bell is not text");
    }

    #[test]
    fn a_prompt_is_drawn_once_when_the_shell_reads_the_file_and_twice_when_the_line_echoes_it() {
        let text = "Escreva o guia.\nUse a seção de instalação.";
        let typed = "$ opencode --prompt \"$(cat '/tmp/p.md')\"\n";
        assert!(
            !drawn_at_least(typed, text, 1),
            "the typed line names the file, not the text"
        );

        let drawn = format!("{typed}Escreva o guia.\nUse a seção de instalação.");
        assert!(drawn_at_least(&drawn, text, 1));

        let echoed = "$ opencode --prompt 'Read /w/p.md and follow the instructions in it.'\n";
        let instruction = "Read /w/p.md and follow the instructions in it.";
        assert!(
            !drawn_at_least(echoed, instruction, 2),
            "the echo alone is not enough"
        );
        let shown = format!("{echoed}Read /w/p.md and follow the\ninstructions in it.");
        assert!(
            drawn_at_least(&shown, instruction, 2),
            "a line break does not hide it"
        );
    }

    #[test]
    fn a_posix_shell_reads_the_prompt_file_into_the_first_message() {
        let first = first_prompt_for("/bin/zsh", "/tmp/lynx-prompts/p.md", "Tarefa", false);

        assert_eq!(first.argument, "\"$(cat /tmp/lynx-prompts/p.md)\"");
        assert_eq!(first.shown, "Tarefa");
        assert!(!first.echoed);
    }

    #[test]
    fn powershell_reads_it_with_get_content_and_cmd_is_told_to_read_the_file() {
        let path = "C:\\Users\\Ana's\\lynx-prompts\\p.md";
        let pwsh = first_prompt_for(
            "C:\\Program Files\\PowerShell\\pwsh.exe",
            path,
            "Tarefa",
            true,
        );
        assert_eq!(
            pwsh.argument,
            "\"$(Get-Content -Raw 'C:\\Users\\Ana''s\\lynx-prompts\\p.md')\""
        );

        let cmd = first_prompt_for("cmd.exe", path, "Tarefa", true);
        assert_eq!(
            cmd.shown,
            format!("Read {path} and follow the instructions in it.")
        );
        assert!(cmd.echoed, "the typed line carries the instruction");
    }

    #[test]
    fn a_shell_nobody_listed_is_told_to_read_the_file() {
        let first = first_prompt_for("/usr/bin/tcsh", "/tmp/p.md", "Tarefa", false);

        assert_eq!(
            first.argument,
            "'Read /tmp/p.md and follow the instructions in it.'"
        );
        assert!(first.echoed);
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
