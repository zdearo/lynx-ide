use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::error::AppError;
use crate::git::PorcelainWorktree;
use crate::git::worktree_manager::{self, BranchDeletion, RepoRefs};
use crate::persistence::AppStateStore;

use super::workspace_models::{RemoveWorktreeResult, Repo, WorktreeInfo, WorktreeMeta};

const MAX_NAME_LENGTH: usize = 100;

/// Repos and worktrees. Git is the source of truth for which worktrees exist;
/// the store only adds what Git cannot tell us (which repos are open, what
/// each worktree was branched from).
#[derive(Clone)]
pub struct WorkspaceService {
    store: Arc<AppStateStore>,
}

impl WorkspaceService {
    pub fn new(store: AppStateStore) -> Self {
        Self {
            store: Arc::new(store),
        }
    }

    pub fn list_repos(&self) -> Vec<Repo> {
        self.store.read(|state| state.repos.clone())
    }

    pub fn add_repo(&self, path: &str) -> Result<Repo, AppError> {
        let given = Path::new(path);
        if !given.is_dir() {
            return Err(AppError::NotAGitRepo(path.to_owned()));
        }

        let worktrees = worktree_manager::list_worktrees(given).map_err(|error| {
            if error.stderr_contains("not a git repository") {
                AppError::NotAGitRepo(path.to_owned())
            } else {
                AppError::Git(error)
            }
        })?;
        let main = worktrees
            .first()
            .ok_or_else(|| AppError::NotAGitRepo(path.to_owned()))?;
        if main.bare {
            return Err(AppError::BareRepo);
        }

        let main_path = path_to_string(&main.path);
        let existing = self.store.read(|state| {
            state
                .repos
                .iter()
                .find(|repo| repo.path == main_path)
                .cloned()
        });
        if let Some(repo) = existing {
            return Ok(repo);
        }

        let repo = Repo {
            id: uuid::Uuid::new_v4().to_string(),
            name: directory_name(&main.path),
            path: main_path,
        };
        self.store.update(|state| state.repos.push(repo.clone()))?;
        Ok(repo)
    }

    /// Forgets the repo; nothing on disk is touched.
    pub fn remove_repo(&self, repo_id: &str) -> Result<(), AppError> {
        self.find_repo(repo_id)?;
        self.store.update(|state| {
            state.repos.retain(|repo| repo.id != repo_id);
            state.worktrees.remove(repo_id);
        })?;
        Ok(())
    }

    pub fn repo_refs(&self, repo_id: &str) -> Result<RepoRefs, AppError> {
        let repo = self.find_repo(repo_id)?;
        Ok(worktree_manager::repo_refs(Path::new(&repo.path))?)
    }

    pub fn list_worktrees(&self, repo_id: &str) -> Result<Vec<WorktreeInfo>, AppError> {
        let repo = self.find_repo(repo_id)?;
        let entries = worktree_manager::list_worktrees(Path::new(&repo.path))?;
        let meta = self.worktree_meta(repo_id);

        Ok(entries
            .iter()
            .enumerate()
            .map(|(index, entry)| to_worktree_info(entry, index == 0, &meta))
            .collect())
    }

    pub fn create_worktree(
        &self,
        repo_id: &str,
        name: &str,
        base_ref: Option<&str>,
    ) -> Result<WorktreeInfo, AppError> {
        let repo = self.find_repo(repo_id)?;
        let repo_path = Path::new(&repo.path);

        let branch = validate_name(repo_path, name)?;
        let destination = worktrees_directory(repo_path).join(directory_slug(&branch)?);
        if destination.exists() {
            return Err(AppError::AlreadyExists(path_to_string(&destination)));
        }

        let base = match base_ref.map(str::trim).filter(|base| !base.is_empty()) {
            Some(base) if base.starts_with('-') => {
                return Err(AppError::InvalidName(format!("{base} is not a valid ref")));
            }
            Some(base) => base.to_owned(),
            None => worktree_manager::repo_refs(repo_path)?.default_base,
        };

        worktree_manager::fetch_start_point(repo_path, &base);
        worktree_manager::add_worktree(repo_path, &branch, &destination, &base).map_err(
            |error| {
                if error.stderr_contains("already exists") {
                    AppError::AlreadyExists(format!("branch {branch}"))
                } else {
                    AppError::Git(error)
                }
            },
        )?;

        // Git reports the resolved path, which is what later listings will show.
        let created = worktree_manager::list_worktrees(repo_path)?
            .into_iter()
            .find(|entry| same_location(&entry.path, &destination))
            .ok_or_else(|| AppError::WorktreeNotFound(path_to_string(&destination)))?;
        let created_path = path_to_string(&created.path);

        self.store.update(|state| {
            state.worktrees.entry(repo.id.clone()).or_default().insert(
                created_path.clone(),
                WorktreeMeta {
                    base_ref: base.clone(),
                },
            );
        })?;

        let meta = self.worktree_meta(repo_id);
        Ok(to_worktree_info(&created, false, &meta))
    }

    pub fn remove_worktree(
        &self,
        repo_id: &str,
        worktree_path: &str,
        force: bool,
    ) -> Result<RemoveWorktreeResult, AppError> {
        let repo = self.find_repo(repo_id)?;
        let repo_path = Path::new(&repo.path);

        let entries = worktree_manager::list_worktrees(repo_path)?;
        let (index, entry) = entries
            .iter()
            .enumerate()
            .find(|(_, entry)| entry.path == Path::new(worktree_path))
            .ok_or_else(|| AppError::WorktreeNotFound(worktree_path.to_owned()))?;
        if index == 0 {
            return Err(AppError::CannotRemoveMainWorktree);
        }

        if entry.prunable {
            // The directory is already gone; only Git's bookkeeping is left.
            worktree_manager::prune_worktrees(repo_path)?;
        } else {
            worktree_manager::remove_worktree(repo_path, &entry.path, force).map_err(|error| {
                if error.stderr_contains("modified or untracked files") {
                    AppError::WorktreeHasChanges
                } else {
                    AppError::Git(error)
                }
            })?;
        }

        let removed_path = path_to_string(&entry.path);
        self.store.update(|state| {
            if let Some(meta) = state.worktrees.get_mut(repo_id) {
                meta.remove(&removed_path);
            }
        })?;

        Ok(delete_worktree_branch(repo_path, entry.branch.as_deref()))
    }

    fn find_repo(&self, repo_id: &str) -> Result<Repo, AppError> {
        self.store
            .read(|state| state.repos.iter().find(|repo| repo.id == repo_id).cloned())
            .ok_or_else(|| AppError::RepoNotFound(repo_id.to_owned()))
    }

    fn worktree_meta(&self, repo_id: &str) -> BTreeMap<String, WorktreeMeta> {
        self.store
            .read(|state| state.worktrees.get(repo_id).cloned().unwrap_or_default())
    }
}

/// A kept branch is not a failure: the worktree is already gone, and an
/// unmerged branch is the user's only copy of that work.
fn delete_worktree_branch(repo_path: &Path, branch: Option<&str>) -> RemoveWorktreeResult {
    let Some(branch) = branch else {
        return RemoveWorktreeResult {
            branch: None,
            branch_deleted: false,
            branch_kept_reason: None,
        };
    };

    let (branch_deleted, branch_kept_reason) =
        match worktree_manager::delete_branch(repo_path, branch) {
            Ok(BranchDeletion::Deleted) => (true, None),
            Ok(BranchDeletion::KeptUnmerged) => (
                false,
                Some("the branch has commits that are not merged".to_owned()),
            ),
            Err(error) => (false, Some(error.to_string())),
        };
    RemoveWorktreeResult {
        branch: Some(branch.to_owned()),
        branch_deleted,
        branch_kept_reason,
    }
}

fn to_worktree_info(
    entry: &PorcelainWorktree,
    is_main: bool,
    meta: &BTreeMap<String, WorktreeMeta>,
) -> WorktreeInfo {
    let path = path_to_string(&entry.path);
    WorktreeInfo {
        base_ref: meta.get(&path).map(|meta| meta.base_ref.clone()),
        name: directory_name(&entry.path),
        path,
        branch: entry.branch.clone(),
        head: entry.head.clone(),
        is_main,
        locked: entry.locked,
        prunable: entry.prunable,
    }
}

/// The name becomes both the branch and (slugified) the directory.
fn validate_name(repo_path: &Path, name: &str) -> Result<String, AppError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::InvalidName("the name is empty".to_owned()));
    }
    if name.len() > MAX_NAME_LENGTH {
        return Err(AppError::InvalidName(format!(
            "the name is longer than {MAX_NAME_LENGTH} characters"
        )));
    }
    // A leading dash would be read as an option; `@{` is reflog syntax.
    if name.starts_with('-') || name.contains("@{") {
        return Err(AppError::InvalidName(format!(
            "{name} is not a valid branch name"
        )));
    }
    let valid = worktree_manager::is_valid_branch_name(repo_path, name)?;
    if !valid {
        return Err(AppError::InvalidName(format!(
            "{name} is not a valid branch name"
        )));
    }
    Ok(name.to_owned())
}

fn directory_slug(name: &str) -> Result<String, AppError> {
    let slug: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '-'
            }
        })
        .collect();
    let slug = slug.trim_matches(['.', '-']).to_owned();
    if slug.is_empty() {
        return Err(AppError::InvalidName(format!(
            "{name} cannot be used as a directory name"
        )));
    }
    Ok(slug)
}

/// Worktrees live next to the repository, never inside it.
fn worktrees_directory(repo_path: &Path) -> PathBuf {
    let parent = repo_path.parent().unwrap_or(repo_path);
    parent.join(format!("{}-worktrees", directory_name(repo_path)))
}

fn same_location(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

fn directory_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path_to_string(path),
        |name| name.to_string_lossy().into_owned(),
    )
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
