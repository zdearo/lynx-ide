//! The app's side of the orchestration: its terminals, worktrees, prompt files and settings, read
//! from the managed services when a call needs them.

use std::sync::Arc;

use tauri::{AppHandle, Manager};

use crate::agents::agent_prompt_file;
use crate::agents::harness::HarnessId;
use crate::agents::launch_line::launch_line;
use crate::browser::{PageOwner, SessionOwners};
use crate::config::ConfigStore;
use crate::config::app_config::LaunchMode;
use crate::terminal::terminal_manager::TerminalManager;
use crate::workspace::workspace_service::WorkspaceService;

use super::service::{Reading, WorkerHost};

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
}

impl WorkerHost for AppWorkers {
    fn worktree_of(&self, session: &str) -> Option<String> {
        match self.owners.get(session)? {
            PageOwner::Workspace { worktree_path } => Some(worktree_path),
            PageOwner::Bot { .. } => None,
        }
    }

    fn new_worktree(&self, from: &str, name: &str) -> Result<String, String> {
        let workspace = self.app.state::<WorkspaceService>();
        let repo = workspace
            .repo_id_of_worktree(from)
            .ok_or_else(|| "Lynx does not know the project this agent works in".to_owned())?;
        workspace
            .create_worktree(&repo, name, None)
            .map(|worktree| worktree.path)
            .map_err(|error| error.to_string())
    }

    fn prompt_file(&self, text: &str) -> Result<String, String> {
        agent_prompt_file::write(text)
            .map(|path| path.to_string_lossy().into_owned())
            .map_err(|error| error.to_string())
    }

    fn launch_line(&self, harness: HarnessId, mode: LaunchMode) -> String {
        launch_line(harness, mode, &self.config.config().agents.launch_arguments)
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

    fn reading(&self, session: &str) -> Result<Reading, String> {
        let terminals = self.terminals();
        let info = terminals.info(session).map_err(|error| error.to_string())?;
        let snapshot = terminals
            .snapshot(session)
            .map_err(|error| error.to_string())?;
        Ok(Reading {
            output: visible_text(&snapshot.data),
            running: info.running,
            status: info.agent_status,
        })
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

#[cfg(test)]
mod tests {
    use super::visible_text;

    #[test]
    fn escape_sequences_are_dropped_and_the_text_kept() {
        assert_eq!(visible_text("\u{1b}[31mred\u{1b}[0m text"), "red text");
        assert_eq!(visible_text("\u{1b}]0;title\u{7}line\r\n"), "line\r\n");
        assert_eq!(visible_text("a\u{7}b"), "ab", "a bell is not text");
    }
}
