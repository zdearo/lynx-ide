use std::sync::Arc;

use tauri::State;

use crate::blocking_task::run_blocking;
use crate::changes::commit_message_prompt::style_text;
use crate::config::ConfigStore;
use crate::error::{AppError, IpcError};
use crate::workspace::workspace_service::WorkspaceService;

use super::github_remote::{CreateRemoteRequest, GitHubAccount, account, create_remote};
use super::pull_request_create::{
    CreatePullRequestRequest, CreatedPullRequest, PullRequestForm, create, form,
};
use super::pull_request_draft::{DraftedPullRequest, draft};
use super::pull_request_lookup::{PullRequest, lookup};

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

/// Opens a pull request's page in the browser. Only `https` links are opened.
#[tauri::command]
#[specta::specta]
pub async fn open_pull_request_url(url: String) -> Result<(), IpcError> {
    run_blocking(move || {
        if !is_https_link(&url) {
            return Err(AppError::InvalidLink);
        }
        open::that(&url).map_err(AppError::OpenLink)
    })
    .await
}

/// A link without spaces or control characters, starting with `https://`.
pub fn is_https_link(url: &str) -> bool {
    url.starts_with("https://")
        && url.len() > "https://".len()
        && !url.chars().any(|c| c.is_whitespace() || c.is_control())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_https_links_without_spaces_are_opened() {
        assert!(is_https_link("https://github.com/acme/app/pull/42"));
        assert!(!is_https_link("http://github.com/acme/app/pull/42"));
        assert!(!is_https_link("file:///etc/passwd"));
        assert!(!is_https_link("https://"));
        assert!(!is_https_link("https://github.com/a b"));
    }
}
