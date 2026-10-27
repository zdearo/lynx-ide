//! The app's side of the commands a bot leaves running: its terminals.

use std::path::Path;

use tauri::{AppHandle, Manager};

use super::bot_processes::CommandHost;
use crate::orchestration::host::visible_text;
use crate::terminal::terminal_manager::TerminalManager;

pub struct AppCommandHost {
    app: AppHandle,
}

impl AppCommandHost {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }

    fn terminals(&self) -> tauri::State<'_, TerminalManager> {
        self.app.state::<TerminalManager>()
    }
}

impl CommandHost for AppCommandHost {
    fn start(&self, cwd: &Path, command: &str) -> Result<String, String> {
        self.terminals()
            .create(&cwd.to_string_lossy(), Some(command), 120, 32)
            .map(|info| info.id)
            .map_err(|error| error.to_string())
    }

    fn running(&self, session: &str) -> bool {
        self.terminals()
            .info(session)
            .is_ok_and(|info| info.running)
    }

    fn output(&self, session: &str) -> String {
        self.terminals()
            .snapshot(session)
            .map(|snapshot| visible_text(&snapshot.data))
            .unwrap_or_default()
    }

    fn stop(&self, session: &str) {
        let _ = self.terminals().close(session);
    }
}
