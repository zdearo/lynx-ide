use std::sync::Arc;

use tauri::State;

use crate::blocking_task::run_blocking;
use crate::config::ConfigStore;
use crate::error::IpcError;
use crate::workspace::workspace_service::WorkspaceService;

use super::branch_sync::{
    BranchSync, PullKind, fetch, force_push_with_lease, pull, push, read as read_branch_sync,
};
use super::change_discard::discard;
use super::change_list::{ChangedFile, list_changes as list};
use super::change_staging::{stage, stage_many, unstage};
use super::commit_changes::{CommitMode, commit, last_message};
use super::commit_history::{CommitEntry, CommitFile, list as list_history, list_files};
use super::commit_message_draft::{DraftedMessage, draft_message};
use super::commit_message_prompt::style_text;
use super::file_diff::{DiffSource, FileDiff, read_diff};

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

/// The two sides of one file's diff for a source: HEAD, the index, a commit or the base branch.
/// `original_path` is the old path of a renamed file.
#[tauri::command]
#[specta::specta]
pub async fn read_file_diff(
    workspace: State<'_, WorkspaceService>,
    worktree_path: String,
    relative_path: String,
    original_path: Option<String>,
    source: DiffSource,
) -> Result<FileDiff, IpcError> {
    let workspace = workspace.inner().clone();
    run_blocking(move || {
        let root = workspace.resolve_worktree(&worktree_path)?;
        read_diff(&root, &relative_path, original_path.as_deref(), &source)
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

/// Commits what is staged with the message the user wrote, or amends the last commit.
/// Amending a commit that is already on the remote needs `allow_pushed`: the user's yes.
#[tauri::command]
#[specta::specta]
pub async fn commit_changes(
    workspace: State<'_, WorkspaceService>,
    worktree_path: String,
    summary: String,
    description: String,
    amend: bool,
    allow_pushed: bool,
) -> Result<(), IpcError> {
    let workspace = workspace.inner().clone();
    let mode = if amend {
        CommitMode::Amend { allow_pushed }
    } else {
        CommitMode::New
    };
    run_blocking(move || {
        let root = workspace.resolve_worktree(&worktree_path)?;
        commit(&root, &summary, &description, mode)
    })
    .await
}

/// The last commit's summary and description, to start an amend from.
#[tauri::command]
#[specta::specta]
pub async fn get_last_commit_message(
    workspace: State<'_, WorkspaceService>,
    worktree_path: String,
) -> Result<LastCommitMessage, IpcError> {
    let workspace = workspace.inner().clone();
    run_blocking(move || {
        let root = workspace.resolve_worktree(&worktree_path)?;
        let (summary, description) = last_message(&root)?;
        Ok(LastCommitMessage {
            summary,
            description,
        })
    })
    .await
}

#[derive(serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LastCommitMessage {
    pub summary: String,
    pub description: String,
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

/// Where the worktree's branch stands against its remote.
#[tauri::command]
#[specta::specta]
pub async fn get_branch_sync(
    workspace: State<'_, WorkspaceService>,
    worktree_path: String,
) -> Result<BranchSync, IpcError> {
    let workspace = workspace.inner().clone();
    run_blocking(move || {
        let root = workspace.resolve_worktree(&worktree_path)?;
        read_branch_sync(&root)
    })
    .await
}

/// Pushes the worktree's branch, publishing it first when it has no upstream. Never forces.
#[tauri::command]
#[specta::specta]
pub async fn push_branch(
    workspace: State<'_, WorkspaceService>,
    worktree_path: String,
) -> Result<(), IpcError> {
    let workspace = workspace.inner().clone();
    run_blocking(move || {
        let root = workspace.resolve_worktree(&worktree_path)?;
        push(&root)
    })
    .await
}

/// The latest commits of the worktree's branch, newest first, with whether the remote has each.
#[tauri::command]
#[specta::specta]
pub async fn list_commits(
    workspace: State<'_, WorkspaceService>,
    worktree_path: String,
    limit: u32,
) -> Result<Vec<CommitEntry>, IpcError> {
    let workspace = workspace.inner().clone();
    run_blocking(move || {
        let root = workspace.resolve_worktree(&worktree_path)?;
        list_history(&root, limit)
    })
    .await
}

/// Throws away the unstaged changes of these files: tracked ones go back to the index, untracked
/// ones are deleted. The interface confirms first.
#[tauri::command]
#[specta::specta]
pub async fn discard_changes(
    workspace: State<'_, WorkspaceService>,
    worktree_path: String,
    relative_paths: Vec<String>,
) -> Result<(), IpcError> {
    let workspace = workspace.inner().clone();
    run_blocking(move || {
        let root = workspace.resolve_worktree(&worktree_path)?;
        discard(&root, &relative_paths)
    })
    .await
}

/// The files one commit touched, against its first parent.
#[tauri::command]
#[specta::specta]
pub async fn list_commit_files(
    workspace: State<'_, WorkspaceService>,
    worktree_path: String,
    hash: String,
) -> Result<Vec<CommitFile>, IpcError> {
    let workspace = workspace.inner().clone();
    run_blocking(move || {
        let root = workspace.resolve_worktree(&worktree_path)?;
        list_files(&root, &hash)
    })
    .await
}

/// Reads the remote's news (refs only), so "behind" is up to date. Runs about once a minute.
#[tauri::command]
#[specta::specta]
pub async fn fetch_remote(
    workspace: State<'_, WorkspaceService>,
    worktree_path: String,
) -> Result<(), IpcError> {
    let workspace = workspace.inner().clone();
    run_blocking(move || {
        let root = workspace.resolve_worktree(&worktree_path)?;
        fetch(&root)
    })
    .await
}

/// Brings the upstream's commits into the branch: a fast-forward, or a rebase that is undone on conflict.
#[tauri::command]
#[specta::specta]
pub async fn pull_branch(
    workspace: State<'_, WorkspaceService>,
    worktree_path: String,
    kind: PullKind,
) -> Result<(), IpcError> {
    let workspace = workspace.inner().clone();
    run_blocking(move || {
        let root = workspace.resolve_worktree(&worktree_path)?;
        pull(&root, kind)
    })
    .await
}

/// Replaces the remote's copy of the branch, guarded by a lease. Only for a branch whose history
/// was rewritten here. The interface confirms first.
#[tauri::command]
#[specta::specta]
pub async fn force_push_branch(
    workspace: State<'_, WorkspaceService>,
    worktree_path: String,
) -> Result<(), IpcError> {
    let workspace = workspace.inner().clone();
    run_blocking(move || {
        let root = workspace.resolve_worktree(&worktree_path)?;
        force_push_with_lease(&root)
    })
    .await
}
