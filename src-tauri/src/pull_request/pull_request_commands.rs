use std::sync::Arc;

use tauri::State;

use crate::changes::commit_message_prompt::style_text;
use crate::config::ConfigStore;
use crate::error::{AppError, IpcError};
use crate::process::blocking_task::run_blocking;
use crate::workspace::workspace_models::RemoveWorktreeResult;
use crate::workspace::workspace_service::WorkspaceService;

use super::github_remote::{CreateRemoteRequest, GitHubAccount, account, create_remote};
use super::pull_request_actions::{MergeMethod, PullRequestAction, merge_methods, run_action};
use super::pull_request_create::{
    CreatePullRequestRequest, CreatedPullRequest, PullRequestForm, create, form,
};
use super::pull_request_discussion::{
    Discussion, add_comment, read as read_discussion, reply_to_thread, set_thread_resolved,
};
use super::pull_request_draft::{DraftedPullRequest, draft};
use super::pull_request_logs::{CheckLog, read as read_check_log};
use super::pull_request_lookup::{PullRequest, PullRequestState, lookup};

/// The pull request of a worktree's branch, or `None` when the branch has none.
#[tauri::command]
#[specta::specta]
pub async fn get_pull_request(
    workspace: State<'_, WorkspaceService>,
    worktree_path: String,
) -> Result<Option<PullRequest>, IpcError> {
    let workspace = workspace.inner().clone();
    run_blocking(move || {
        let root = workspace.resolve_worktree(&worktree_path)?;
        lookup(&root).map_err(AppError::from)
    })
    .await
}

/// Removes the worktree of a branch whose pull request is merged, and the branch with it.
/// GitHub is asked here, not trusted from the screen: the pull request must be merged, and the
/// branch is deleted only if it still points at the commit that was merged. Uncommitted changes
/// stop it, as in any safe removal.
#[tauri::command]
#[specta::specta]
pub async fn clean_up_merged_worktree(
    workspace: State<'_, WorkspaceService>,
    repo_id: String,
    worktree_path: String,
) -> Result<RemoveWorktreeResult, IpcError> {
    let workspace = workspace.inner().clone();
    run_blocking(move || {
        let root = workspace.resolve_worktree(&worktree_path)?;
        let pull_request = lookup(&root)
            .map_err(AppError::from)?
            .filter(|pr| pr.state == PullRequestState::Merged && !pr.head_sha.is_empty())
            .ok_or(AppError::ChangeRefused("the pull request is not merged"))?;
        workspace.remove_merged_worktree(&repo_id, &worktree_path, &pull_request.head_sha)
    })
    .await
}

/// The login and organizations `gh` is signed in with, for the account picker of the remote dialog.
#[tauri::command]
#[specta::specta]
pub async fn get_github_account() -> Result<GitHubAccount, IpcError> {
    run_blocking(|| account().map_err(AppError::from)).await
}

/// Creates the GitHub repository for a local one that has no remote, and connects it as `origin`.
#[tauri::command]
#[specta::specta]
pub async fn create_github_remote(
    workspace: State<'_, WorkspaceService>,
    worktree_path: String,
    request: CreateRemoteRequest,
) -> Result<(), IpcError> {
    let workspace = workspace.inner().clone();
    run_blocking(move || {
        let root = workspace.resolve_worktree(&worktree_path)?;
        create_remote(&root, &request).map_err(AppError::from)
    })
    .await
}

/// What the create-pull-request form starts from: the bases, the default one and a title.
#[tauri::command]
#[specta::specta]
pub async fn get_pull_request_form(
    workspace: State<'_, WorkspaceService>,
    worktree_path: String,
) -> Result<PullRequestForm, IpcError> {
    let workspace = workspace.inner().clone();
    run_blocking(move || {
        let root = workspace.resolve_worktree(&worktree_path)?;
        form(&root)
    })
    .await
}

/// A title and description from the user's agent CLI. Nothing is created. The agent and the title
/// style come from the Git settings.
#[tauri::command]
#[specta::specta]
pub async fn draft_pull_request_text(
    workspace: State<'_, WorkspaceService>,
    store: State<'_, Arc<ConfigStore>>,
    worktree_path: String,
    base: String,
) -> Result<DraftedPullRequest, IpcError> {
    let workspace = workspace.inner().clone();
    let git = store.config().git;
    let style = style_text(
        git.commit_message_style,
        git.commit_message_instructions.as_deref(),
    )
    .to_owned();
    run_blocking(move || {
        let root = workspace.resolve_worktree(&worktree_path)?;
        draft(&root, git.commit_message_agent, &style, &base)
    })
    .await
}

/// Pushes the branch if the remote lacks it, then opens the pull request.
#[tauri::command]
#[specta::specta]
pub async fn create_pull_request(
    workspace: State<'_, WorkspaceService>,
    worktree_path: String,
    request: CreatePullRequestRequest,
) -> Result<CreatedPullRequest, IpcError> {
    let workspace = workspace.inner().clone();
    run_blocking(move || {
        let root = workspace.resolve_worktree(&worktree_path)?;
        create(&root, &request)
    })
    .await
}

/// The merge methods the repository allows, for the merge button's picker.
#[tauri::command]
#[specta::specta]
pub async fn get_merge_methods(
    workspace: State<'_, WorkspaceService>,
    worktree_path: String,
) -> Result<Vec<MergeMethod>, IpcError> {
    let workspace = workspace.inner().clone();
    run_blocking(move || {
        let root = workspace.resolve_worktree(&worktree_path)?;
        merge_methods(&root).map_err(AppError::from)
    })
    .await
}

/// Merges, closes, reopens or changes the draft state of a pull request. The interface confirms a merge.
#[tauri::command]
#[specta::specta]
pub async fn run_pull_request_action(
    workspace: State<'_, WorkspaceService>,
    worktree_path: String,
    number: u32,
    action: PullRequestAction,
) -> Result<(), IpcError> {
    let workspace = workspace.inner().clone();
    run_blocking(move || {
        let root = workspace.resolve_worktree(&worktree_path)?;
        run_action(&root, number, &action).map_err(AppError::from)
    })
    .await
}

/// The conversation, reviews and inline threads of a pull request.
#[tauri::command]
#[specta::specta]
pub async fn get_pull_request_discussion(
    workspace: State<'_, WorkspaceService>,
    worktree_path: String,
    number: u32,
) -> Result<Discussion, IpcError> {
    let workspace = workspace.inner().clone();
    run_blocking(move || {
        let root = workspace.resolve_worktree(&worktree_path)?;
        read_discussion(&root, number).map_err(AppError::from)
    })
    .await
}

/// A new comment on the conversation of a pull request.
#[tauri::command]
#[specta::specta]
pub async fn comment_on_pull_request(
    workspace: State<'_, WorkspaceService>,
    worktree_path: String,
    number: u32,
    body: String,
) -> Result<(), IpcError> {
    let workspace = workspace.inner().clone();
    run_blocking(move || {
        let root = workspace.resolve_worktree(&worktree_path)?;
        add_comment(&root, number, &body).map_err(AppError::from)
    })
    .await
}

/// A reply in an inline review thread.
#[tauri::command]
#[specta::specta]
pub async fn reply_to_review_thread(
    workspace: State<'_, WorkspaceService>,
    worktree_path: String,
    thread_id: String,
    body: String,
) -> Result<(), IpcError> {
    let workspace = workspace.inner().clone();
    run_blocking(move || {
        let root = workspace.resolve_worktree(&worktree_path)?;
        reply_to_thread(&root, &thread_id, &body).map_err(AppError::from)
    })
    .await
}

/// Marks an inline review thread resolved, or open again.
#[tauri::command]
#[specta::specta]
pub async fn resolve_review_thread(
    workspace: State<'_, WorkspaceService>,
    worktree_path: String,
    thread_id: String,
    resolved: bool,
) -> Result<(), IpcError> {
    let workspace = workspace.inner().clone();
    run_blocking(move || {
        let root = workspace.resolve_worktree(&worktree_path)?;
        set_thread_resolved(&root, &thread_id, resolved).map_err(AppError::from)
    })
    .await
}

/// The log of the GitHub Actions job a check points at. Only its end is returned.
#[tauri::command]
#[specta::specta]
pub async fn get_check_log(
    workspace: State<'_, WorkspaceService>,
    worktree_path: String,
    details_url: String,
    failed_only: bool,
) -> Result<CheckLog, IpcError> {
    let workspace = workspace.inner().clone();
    run_blocking(move || {
        let root = workspace.resolve_worktree(&worktree_path)?;
        read_check_log(&root, &details_url, failed_only).map_err(AppError::from)
    })
    .await
}
