use tauri::State;

use crate::blocking_task::run_blocking;
use crate::error::{AppError, IpcError};

use super::daemon_host::DaemonStatus;
use super::terminal_manager::{ResumableSession, TerminalManager};
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
pub async fn pty_daemon_status(
    manager: State<'_, TerminalManager>,
) -> Result<DaemonStatus, IpcError> {
    Ok(manager.daemon_status())
}

/// The memory the daemon's sessions hold, in MiB; None when the daemon is off or does not answer.
#[tauri::command]
#[specta::specta]
pub async fn pty_daemon_memory(
    manager: State<'_, TerminalManager>,
) -> Result<Option<u32>, IpcError> {
    let manager = manager.inner().clone();
    run_blocking(move || Ok(manager.daemon_memory_mib())).await
}

/// Freezes a terminal of the background process that the user is not using. Refused, with the
/// reason as the message, while it cannot sleep.
#[tauri::command]
#[specta::specta]
pub async fn hibernate_terminal(
    manager: State<'_, TerminalManager>,
    id: String,
) -> Result<(), IpcError> {
    let manager = manager.inner().clone();
    run_blocking(move || Ok(manager.hibernate(&id)?)).await
}

/// Takes an ended agent conversation off the resumable list. The agent keeps the conversation.
#[tauri::command]
#[specta::specta]
pub async fn forget_resumable_session(
    manager: State<'_, TerminalManager>,
    id: String,
) -> Result<(), IpcError> {
    let manager = manager.inner().clone();
    run_blocking(move || Ok(manager.forget_resumable(&id)?)).await
}

#[tauri::command]
#[specta::specta]
pub async fn list_resumable_sessions(
    manager: State<'_, TerminalManager>,
    worktree_path: String,
) -> Result<Vec<ResumableSession>, IpcError> {
    Ok(manager.resumable(&worktree_path))
}

/// Chooses what closing the app does with the daemon's sessions, before the window closes.
#[tauri::command]
#[specta::specta]
pub async fn prepare_quit(
    manager: State<'_, TerminalManager>,
    keep_sessions: bool,
) -> Result<(), IpcError> {
    manager.set_keep_on_exit(keep_sessions);
    Ok(())
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
pub async fn mark_agent_seen(
    manager: State<'_, TerminalManager>,
    id: String,
) -> Result<(), IpcError> {
    manager
        .mark_seen(&id)
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
