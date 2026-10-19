use std::sync::Arc;

use tauri::State;

use crate::blocking_task::run_blocking;
use crate::config::ConfigStore;
use crate::error::IpcError;
use crate::workspace::workspace_service::WorkspaceService;

use super::change_list::{ChangedFile, list_changes as list};
use super::change_staging::{stage, stage_many, unstage};
use super::commit_changes::commit;
use super::commit_message_draft::{DraftedMessage, draft_message};
use super::commit_message_prompt::style_text;
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

/// Stages one changed file: a deletion stages the removal, an untracked file gets tracked.
#[tauri::command]
#[specta::specta]
pub async fn stage_change(
    workspace: State<'_, WorkspaceService>,
    worktree_path: String,
    relative_path: String,
) -> Result<(), IpcError> {
    let workspace = workspace.inner().clone();
    run_blocking(move || {
        let root = workspace.resolve_worktree(&worktree_path)?;
        stage(&root, &relative_path)
    })
    .await
}

/// Takes one file out of the index. `original_path` is the old path of a renamed file.
#[tauri::command]
#[specta::specta]
pub async fn unstage_change(
    workspace: State<'_, WorkspaceService>,
    worktree_path: String,
    relative_path: String,
    original_path: Option<String>,
) -> Result<(), IpcError> {
    let workspace = workspace.inner().clone();
    run_blocking(move || {
        let root = workspace.resolve_worktree(&worktree_path)?;
        unstage(&root, &relative_path, original_path.as_deref())
    })
    .await
}

/// Commits what is staged with the message the user wrote.
#[tauri::command]
#[specta::specta]
pub async fn commit_changes(
    workspace: State<'_, WorkspaceService>,
    worktree_path: String,
    summary: String,
    description: String,
) -> Result<(), IpcError> {
    let workspace = workspace.inner().clone();
    run_blocking(move || {
        let root = workspace.resolve_worktree(&worktree_path)?;
        commit(&root, &summary, &description)
    })
    .await
}

/// A commit message written by the user's agent CLI from what is staged. Nothing is committed.
/// The agent and the style come from the Git settings, read when the draft starts.
#[tauri::command]
#[specta::specta]
pub async fn draft_commit_message(
    workspace: State<'_, WorkspaceService>,
    store: State<'_, Arc<ConfigStore>>,
    worktree_path: String,
) -> Result<DraftedMessage, IpcError> {
    let workspace = workspace.inner().clone();
    let git = store.config().git;
    let style = style_text(
        git.commit_message_style,
        git.commit_message_instructions.as_deref(),
    )
    .to_owned();
    run_blocking(move || {
        let root = workspace.resolve_worktree(&worktree_path)?;
        draft_message(&root, git.commit_message_agent, &style).map_err(crate::error::AppError::from)
    })
    .await
}

/// Stages several changed files in one step: a section's "stage all" button.
#[tauri::command]
#[specta::specta]
pub async fn stage_changes(
    workspace: State<'_, WorkspaceService>,
    worktree_path: String,
    relative_paths: Vec<String>,
) -> Result<(), IpcError> {
    let workspace = workspace.inner().clone();
    run_blocking(move || {
        let root = workspace.resolve_worktree(&worktree_path)?;
        stage_many(&root, &relative_paths)
    })
    .await
}
