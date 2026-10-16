//! Worktree and ref operations. Every command here works on Git 2.25 and later.

use std::path::Path;

use serde::Serialize;
use specta::Type;

use super::git_command::{GitError, git};
use super::worktree_porcelain::{PorcelainWorktree, parse_worktree_list};

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RepoRefs {
    /// Ref new worktrees branch from unless the user picks another one.
    pub default_base: String,
    /// Local branches and remote-tracking branches, e.g. `main`, `origin/main`.
    pub refs: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum BranchDeletion {
    Deleted,
    KeptUnmerged,
}

pub fn list_worktrees(repo: &Path) -> Result<Vec<PorcelainWorktree>, GitError> {
    let output = git(repo).args(["worktree", "list", "--porcelain"]).run()?;
    Ok(parse_worktree_list(&output))
}

pub fn repo_refs(repo: &Path) -> Result<RepoRefs, GitError> {
    let output = git(repo)
        .args([
            "for-each-ref",
            "--format=%(refname)",
            "refs/heads",
            "refs/remotes",
        ])
        .run()?;

    // Full names, because `%(refname:short)` renders `origin/HEAD` as plain `origin`.
    let mut refs: Vec<String> = output
        .lines()
        .filter(|name| !name.ends_with("/HEAD"))
        .filter_map(|name| {
            name.strip_prefix("refs/heads/")
                .or_else(|| name.strip_prefix("refs/remotes/"))
        })
        .map(str::to_owned)
        .collect();
    refs.sort();

    Ok(RepoRefs {
        default_base: default_base_ref(repo),
        refs,
    })
}

fn default_base_ref(repo: &Path) -> String {
    let remote_default = git(repo)
        .args(["symbolic-ref", "--short", "refs/remotes/origin/HEAD"])
        .run();
    if let Ok(name) = remote_default
        && !name.is_empty()
    {
        return name;
    }

    match git(repo).args(["branch", "--show-current"]).run() {
        Ok(name) if !name.is_empty() => name,
        _ => "HEAD".to_owned(),
    }
}

/// Best effort: being offline must not block creating a worktree from the
/// remote-tracking ref we already have.
pub fn fetch_start_point(repo: &Path, start_point: &str) {
    let Ok(remotes) = git(repo).arg("remote").run() else {
        return;
    };
    let Some((remote, branch)) = start_point.split_once('/') else {
        return;
    };
    if !remotes.lines().any(|name| name == remote) {
        return;
    }

    let _ = git(repo)
        .args([
            "-c",
            "gc.auto=0",
            "-c",
            "maintenance.auto=false",
            "fetch",
            "--quiet",
        ])
        .args([remote, branch])
        .run();
}

pub fn is_valid_branch_name(repo: &Path, name: &str) -> Result<bool, GitError> {
    match git(repo)
        .args(["check-ref-format", "--branch"])
        .arg(name)
        .run()
    {
        Ok(_) => Ok(true),
        Err(GitError::Failed { .. }) => Ok(false),
        Err(other) => Err(other),
    }
}

pub fn add_worktree(
    repo: &Path,
    branch: &str,
    destination: &Path,
    start_point: &str,
) -> Result<(), GitError> {
    // `--no-track` keeps a branch cut from `origin/main` from tracking (and
    // later pushing to) `main`.
    git(repo)
        .args(["worktree", "add", "--no-track", "-b"])
        .arg(branch)
        .arg(destination)
        .arg(start_point)
        .run()
        .map(|_| ())
}

pub fn remove_worktree(repo: &Path, worktree: &Path, force: bool) -> Result<(), GitError> {
    let mut command = git(repo).args(["worktree", "remove"]);
    if force {
        command = command.arg("--force");
    }
    command.arg(worktree).run().map(|_| ())
}

/// Drops Git's records of worktrees whose directory no longer exists.
pub fn prune_worktrees(repo: &Path) -> Result<(), GitError> {
    git(repo).args(["worktree", "prune"]).run().map(|_| ())
}

/// Uses `branch -d`, which refuses to drop commits that exist nowhere else.
pub fn delete_branch(repo: &Path, branch: &str) -> Result<BranchDeletion, GitError> {
    match git(repo).args(["branch", "-d"]).arg(branch).run() {
        Ok(_) => Ok(BranchDeletion::Deleted),
        Err(error) if error.stderr_contains("not fully merged") => Ok(BranchDeletion::KeptUnmerged),
        Err(error) => Err(error),
    }
}
