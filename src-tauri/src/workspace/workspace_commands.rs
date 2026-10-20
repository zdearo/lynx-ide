use tauri::State;

use crate::blocking_task::run_blocking;
use crate::error::IpcError;
use crate::git::worktree_manager::RepoRefs;

use super::linked_folders::LinkedFolderPreview;
use super::workspace_models::{Folder, RemoveWorktreeResult, Repo, WorktreeInfo};
use super::workspace_service::WorkspaceService;

#[tauri::command]
#[specta::specta]
pub async fn list_repos(service: State<'_, WorkspaceService>) -> Result<Vec<Repo>, IpcError> {
    let service = service.inner().clone();
    run_blocking(move || service.list_repos()).await
}

#[tauri::command]
#[specta::specta]
pub async fn add_repo(
    service: State<'_, WorkspaceService>,
    path: String,
) -> Result<Repo, IpcError> {
    let service = service.inner().clone();
    run_blocking(move || service.add_repo(&path)).await
}

#[tauri::command]
#[specta::specta]
pub async fn create_project(
    service: State<'_, WorkspaceService>,
    parent_path: String,
    name: String,
) -> Result<Repo, IpcError> {
    let service = service.inner().clone();
    run_blocking(move || service.create_project(&parent_path, &name)).await
}

#[tauri::command]
#[specta::specta]
pub async fn remove_repo(
    service: State<'_, WorkspaceService>,
    repo_id: String,
) -> Result<(), IpcError> {
    let service = service.inner().clone();
    run_blocking(move || service.remove_repo(&repo_id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn list_worktrees(
    service: State<'_, WorkspaceService>,
    repo_id: String,
) -> Result<Vec<WorktreeInfo>, IpcError> {
    let service = service.inner().clone();
    run_blocking(move || service.list_worktrees(&repo_id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn create_worktree(
    service: State<'_, WorkspaceService>,
    repo_id: String,
    name: String,
    base_ref: Option<String>,
) -> Result<WorktreeInfo, IpcError> {
    let service = service.inner().clone();
    run_blocking(move || service.create_worktree(&repo_id, &name, base_ref.as_deref())).await
}

#[tauri::command]
#[specta::specta]
pub async fn remove_worktree(
    service: State<'_, WorkspaceService>,
    repo_id: String,
    worktree_path: String,
    force: bool,
) -> Result<RemoveWorktreeResult, IpcError> {
    let service = service.inner().clone();
    run_blocking(move || service.remove_worktree(&repo_id, &worktree_path, force)).await
}

#[tauri::command]
#[specta::specta]
pub async fn repo_refs(
    service: State<'_, WorkspaceService>,
    repo_id: String,
) -> Result<RepoRefs, IpcError> {
    let service = service.inner().clone();
    run_blocking(move || service.repo_refs(&repo_id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn list_folders(service: State<'_, WorkspaceService>) -> Result<Vec<Folder>, IpcError> {
    let service = service.inner().clone();
    run_blocking(move || service.list_folders()).await
}

#[tauri::command]
#[specta::specta]
pub async fn create_folder(
    service: State<'_, WorkspaceService>,
    name: String,
    parent_id: Option<String>,
    source_path: Option<String>,
) -> Result<Folder, IpcError> {
    let service = service.inner().clone();
    run_blocking(move || match source_path {
        Some(path) => service.link_folder(&name, parent_id.as_deref(), &path),
        None => service.create_folder(&name, parent_id.as_deref()),
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn preview_linked_folder(
    service: State<'_, WorkspaceService>,
    path: String,
) -> Result<LinkedFolderPreview, IpcError> {
    let service = service.inner().clone();
    run_blocking(move || service.preview_linked_folder(&path)).await
}

/// Reads the linked folders again (all, or just `folder_id`). True when something changed.
#[tauri::command]
#[specta::specta]
pub async fn sync_linked_folders(
    service: State<'_, WorkspaceService>,
    folder_id: Option<String>,
) -> Result<bool, IpcError> {
    let service = service.inner().clone();
    run_blocking(move || service.sync_linked_folders(folder_id.as_deref())).await
}

#[tauri::command]
#[specta::specta]
pub async fn unlink_folder(
    service: State<'_, WorkspaceService>,
    folder_id: String,
) -> Result<(), IpcError> {
    let service = service.inner().clone();
    run_blocking(move || service.unlink_folder(&folder_id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn rename_folder(
    service: State<'_, WorkspaceService>,
    folder_id: String,
    name: String,
) -> Result<Folder, IpcError> {
    let service = service.inner().clone();
    run_blocking(move || service.rename_folder(&folder_id, &name)).await
}

#[tauri::command]
#[specta::specta]
pub async fn move_folder(
    service: State<'_, WorkspaceService>,
    folder_id: String,
    parent_id: Option<String>,
) -> Result<(), IpcError> {
    let service = service.inner().clone();
    run_blocking(move || service.move_folder(&folder_id, parent_id.as_deref())).await
}

#[tauri::command]
#[specta::specta]
pub async fn delete_folder(
    service: State<'_, WorkspaceService>,
    folder_id: String,
) -> Result<(), IpcError> {
    let service = service.inner().clone();
    run_blocking(move || service.delete_folder(&folder_id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn set_folder_collapsed(
    service: State<'_, WorkspaceService>,
    folder_id: String,
    collapsed: bool,
) -> Result<(), IpcError> {
    let service = service.inner().clone();
    run_blocking(move || service.set_folder_collapsed(&folder_id, collapsed)).await
}

#[tauri::command]
#[specta::specta]
pub async fn move_repo(
    service: State<'_, WorkspaceService>,
    repo_id: String,
    folder_id: Option<String>,
) -> Result<(), IpcError> {
    let service = service.inner().clone();
    run_blocking(move || service.move_repo(&repo_id, folder_id.as_deref())).await
}
