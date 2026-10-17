use tauri::State;

use crate::blocking_task::run_blocking;
use crate::error::IpcError;
use crate::workspace::workspace_service::WorkspaceService;

use super::change_list::{ChangedFile, list_changes as list};
use super::file_diff::{FileDiff, read_diff};

/// The files with changes in a worktree: staged, unstaged and untracked.
#[tauri::command]
#[specta::specta]
pub async fn list_changes(
    workspace: State<'_, WorkspaceService>,
    worktree_path: String,
) -> Result<Vec<ChangedFile>, IpcError> {
    let workspace = workspace.inner().clone();
    run_blocking(move || {
        let root = workspace.resolve_worktree(&worktree_path)?;
        list(&root).map_err(crate::error::AppError::from)
    })
    .await
}

/// HEAD and the working tree for one file. `original_path` is the old path of a renamed file.
#[tauri::command]
#[specta::specta]
pub async fn read_file_diff(
    workspace: State<'_, WorkspaceService>,
    worktree_path: String,
    relative_path: String,
    original_path: Option<String>,
) -> Result<FileDiff, IpcError> {
    let workspace = workspace.inner().clone();
    run_blocking(move || {
        let root = workspace.resolve_worktree(&worktree_path)?;
        read_diff(&root, &relative_path, original_path.as_deref())
    })
    .await
}
