//! The app's side of the orchestration: its terminals, worktrees, prompt files and settings, read
//! from the managed services when a call needs them.

use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Manager};
use tauri_specta::Event;

use crate::agents::activity::AgentStatus;
use crate::agents::agent_prompt_file;
use crate::agents::harness::HarnessId;
use crate::agents::launch_line::launch_line;
use crate::bots::bot_bridge::{bot_of_session, parse_session};
use crate::bots::bot_chat::BotChat;
use crate::browser::{PageOwner, SessionOwners};
use crate::config::ConfigStore;
use crate::config::app_config::{LaunchMode, OrchestrationConfig};
use crate::terminal::terminal_manager::TerminalManager;
use crate::workspace::workspace_events::WorktreeCreated;
use crate::workspace::workspace_service::WorkspaceService;

use super::service::WorkerHost;

/// What a bot is told when something in its runs changed: it looks at its workers and tells the person.
const WAKE_NOTE: &str = "Algo mudou nas suas tarefas: um worker terminou, precisa de uma resposta ou parou. Chame lynx_check com waitSeconds 0 para ver o que aconteceu e relate à pessoa.";

/// How often, and how many times, a worker's screen is looked at for its prompt before giving up (30 s).
const SUBMIT_POLL: Duration = Duration::from_millis(500);
const SUBMIT_TRIES: u32 = 60;

/// Workers are terminals of the app, in worktrees of the app's projects.
pub struct AppWorkers {
    app: AppHandle,
    owners: SessionOwners,
    config: Arc<ConfigStore>,
}

impl AppWorkers {
    pub fn new(app: AppHandle, owners: SessionOwners, config: Arc<ConfigStore>) -> Self {
        Self {
            app,
            owners,
            config,
        }
    }

    fn terminals(&self) -> tauri::State<'_, TerminalManager> {
        self.app.state::<TerminalManager>()
    }

    /// A bot works in the first project it was let read: its main folder is where its runs start.
    fn bot_project(&self, bot: &str) -> Option<String> {
        let chat = self.app.state::<Arc<BotChat>>();
        // A project where it may start agents wins over one it only reads.
        let reads = chat.grants(bot).ok()?;
        let starts = chat.worker_grants(bot).unwrap_or_default();
        let first = reads
            .iter()
            .find(|read| starts.iter().any(|start| start.id == read.id))
            .or_else(|| reads.first())?;
        self.app
            .state::<WorkspaceService>()
            .list_repos()
            .ok()?
            .into_iter()
            .find(|repo| repo.id == first.id)
            .map(|repo| repo.path)
    }
}

impl WorkerHost for AppWorkers {
    fn settings(&self) -> OrchestrationConfig {
        self.config.config().orchestration
    }

    fn worktree_of(&self, session: &str) -> Option<String> {
        if let Some(bot) = bot_of_session(session) {
            return self.bot_project(bot);
        }
        match self.owners.get(session)? {
            PageOwner::Workspace { worktree_path } => Some(worktree_path),
            PageOwner::Bot { .. } => None,
        }
    }

    fn notify(&self, coordinator: &str) {
        let Some((bot, label)) = parse_session(coordinator) else {
            return;
        };
        let chat = Arc::clone(&*self.app.state::<Arc<BotChat>>());
        let (bot, label) = (bot.to_owned(), label.to_owned());
        chat.wake(&bot, &label, WAKE_NOTE);
        BotChat::kick(chat, bot, label);
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
        self.terminals()
            .write(session, &format!("{text}\r"))
            .map_err(|error| error.to_string())
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
        let _ = self.terminals().close(session);
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
    use super::{drawn_twice, visible_text};

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
}
