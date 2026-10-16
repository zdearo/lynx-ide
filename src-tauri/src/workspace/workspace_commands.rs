use tauri::State;

use crate::blocking_task::run_blocking;
use crate::error::IpcError;
use crate::git::worktree_manager::RepoRefs;

use super::workspace_models::{RemoveWorktreeResult, Repo, WorktreeInfo};
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
