//! The repositories a bot may read, as Projects lists them: each registered repository, with the folder
//! of its main worktree and of every other worktree.

use std::path::PathBuf;

use tauri::{AppHandle, Manager};

use crate::bots::bot_access::{RegisteredRepos, RepoRef};

use super::workspace_service::WorkspaceService;

pub struct RegisteredWorkspaces(pub AppHandle);

impl RegisteredRepos for RegisteredWorkspaces {
    fn repos(&self) -> Vec<RepoRef> {
        let service = self.0.state::<WorkspaceService>();
        service
            .list_repos()
            .unwrap_or_default()
            .into_iter()
            .map(|repo| RepoRef {
                id: repo.id,
                name: repo.name,
            })
            .collect()
    }

    fn folders_of(&self, repo_id: &str) -> Vec<PathBuf> {
        let service = self.0.state::<WorkspaceService>();
        service
            .list_worktrees(repo_id)
            .unwrap_or_default()
            .into_iter()
            .map(|worktree| PathBuf::from(worktree.path))
            .collect()
    }
}
