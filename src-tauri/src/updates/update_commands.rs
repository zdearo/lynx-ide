use std::process::Command;
use std::sync::Arc;

use tauri::{AppHandle, Manager, State};

use crate::error::{AppError, IpcError};
use crate::process::app_image_environment::{clean_command, launch_executable};
use crate::terminal::terminal_manager::TerminalManager;

use super::update_service::UpdateService;
use super::update_state::UpdateStatus;

#[tauri::command]
#[specta::specta]
pub async fn get_update_status(
    service: State<'_, Arc<UpdateService>>,
) -> Result<UpdateStatus, IpcError> {
    Ok(service.status())
}

/// Looks for a newer version now. A failed check comes back as the status, not as an error.
#[tauri::command]
#[specta::specta]
pub async fn check_for_updates(
    service: State<'_, Arc<UpdateService>>,
) -> Result<UpdateStatus, IpcError> {
    let service = Arc::clone(service.inner());
    Ok(service.check().await)
}

/// Downloads and installs the version the last check found.
#[tauri::command]
#[specta::specta]
pub async fn install_update(
    service: State<'_, Arc<UpdateService>>,
) -> Result<UpdateStatus, IpcError> {
    let service = Arc::clone(service.inner());
    Ok(service.install().await.map_err(AppError::from)?)
}

/// Restarts Lynx so the installed version runs. Terminals inside the app close with it; the
/// daemon's keep running, and the new version rejoins them.
#[tauri::command]
#[specta::specta]
pub fn restart_to_update(app: AppHandle) {
    let terminals = app.state::<TerminalManager>();
    terminals.set_keep_on_exit(true);
    terminals.quit();
    // An AppImage runs from a mount that is gone once the app exits, so the new version starts from
    // the file itself, with the AppImage's own variables left out.
    if let Some(executable) = launch_executable(std::env::current_exe().ok()) {
        let mut command = Command::new(executable);
        clean_command(&mut command);
        if command.spawn().is_ok() {
            app.exit(0);
            return;
        }
    }
    app.restart()
}
