use tauri::State;

use crate::blocking_task::run_blocking;
use crate::error::{AppError, IpcError};

use super::terminal_manager::TerminalManager;
use super::terminal_session::{TerminalInfo, TerminalSnapshot};

#[tauri::command]
#[specta::specta]
pub async fn create_terminal(
    manager: State<'_, TerminalManager>,
    worktree_path: String,
    command: Option<String>,
    cols: u16,
    rows: u16,
) -> Result<TerminalInfo, IpcError> {
    let manager = manager.inner().clone();
    run_blocking(move || Ok(manager.create(&worktree_path, command.as_deref(), cols, rows)?)).await
}

#[tauri::command]
#[specta::specta]
pub async fn list_terminals(
    manager: State<'_, TerminalManager>,
) -> Result<Vec<TerminalInfo>, IpcError> {
    Ok(manager.list())
}

#[tauri::command]
#[specta::specta]
pub async fn terminal_snapshot(
    manager: State<'_, TerminalManager>,
    id: String,
) -> Result<TerminalSnapshot, IpcError> {
    manager
        .snapshot(&id)
        .map_err(|error| AppError::from(error).into())
}

#[tauri::command]
#[specta::specta]
pub async fn write_terminal(
    manager: State<'_, TerminalManager>,
    id: String,
    data: String,
) -> Result<(), IpcError> {
    let manager = manager.inner().clone();
    // Writes can block when the program is not reading, so they stay off the async runtime.
    run_blocking(move || Ok(manager.write(&id, &data)?)).await
}

#[tauri::command]
#[specta::specta]
pub async fn resize_terminal(
    manager: State<'_, TerminalManager>,
    id: String,
    cols: u16,
    rows: u16,
) -> Result<(), IpcError> {
    manager
        .resize(&id, cols, rows)
        .map_err(|error| AppError::from(error).into())
}

#[tauri::command]
#[specta::specta]
pub async fn close_terminal(
    manager: State<'_, TerminalManager>,
    id: String,
) -> Result<(), IpcError> {
    manager
        .close(&id)
        .map_err(|error| AppError::from(error).into())
}
