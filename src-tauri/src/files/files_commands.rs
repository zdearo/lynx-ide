use tauri::State;

use crate::blocking_task::run_blocking;
use crate::error::{AppError, IpcError};
use crate::workspace::workspace_service::WorkspaceService;

use super::directory_listing::{DirectoryListing, list_directory as list};
use super::files_error::FilesError;
use super::path_inside_worktree::resolve_inside;

/// One folder of a worktree. `relative_path` is empty for the root and uses `/`.
#[tauri::command]
#[specta::specta]
pub async fn list_directory(
    workspace: State<'_, WorkspaceService>,
    worktree_path: String,
    relative_path: String,
) -> Result<DirectoryListing, IpcError> {
    let workspace = workspace.inner().clone();
    run_blocking(move || {
        let root = workspace.resolve_worktree(&worktree_path)?;
        Ok(list(&root, &relative_path)?)
    })
    .await
}

/// Opens the folder that holds the item in the system file manager.
#[tauri::command]
#[specta::specta]
pub async fn reveal_in_file_manager(
    workspace: State<'_, WorkspaceService>,
    worktree_path: String,
    relative_path: String,
) -> Result<(), IpcError> {
    let workspace = workspace.inner().clone();
    run_blocking(move || {
        let root = workspace.resolve_worktree(&worktree_path)?;
        let target = resolve_inside(&root, &relative_path)?;
        let folder = if target.is_dir() {
            target
        } else {
            target
                .parent()
                .map_or_else(|| root.clone(), std::path::Path::to_path_buf)
        };
        open::that(&folder).map_err(|source| {
            AppError::from(FilesError::Io {
                path: folder.display().to_string(),
                source,
            })
        })
    })
    .await
}
