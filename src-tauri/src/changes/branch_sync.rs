//! Where the current branch stands against its remote, and pushing it. Never a force push.

use std::path::Path;

use serde::Serialize;
use specta::Type;

use crate::error::AppError;
use crate::git::git_command::git;

/// A branch against its remote. `upstream` is `None` for a branch that was never pushed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BranchSync {
    /// `None` on a detached HEAD.
    pub branch: Option<String>,
    pub upstream: Option<String>,
    /// Commits the branch has that its upstream does not: what a push would send.
    pub ahead: u32,
    pub behind: u32,
    /// The remote a push goes to: `origin` when there is one, else the first. `None` without remotes.
    pub remote: Option<String>,
}

struct Header {
    branch: Option<String>,
    upstream: Option<String>,
    ahead: u32,
    behind: u32,
}

/// Reads the `# branch.*` lines of `git status --porcelain=v2 --branch`.
fn parse_header(status: &str) -> Header {
    let mut header = Header {
        branch: None,
        upstream: None,
        ahead: 0,
        behind: 0,
    };
    for line in status.lines() {
        if let Some(name) = line.strip_prefix("# branch.head ") {
            header.branch = (name != "(detached)").then(|| name.to_owned());
        } else if let Some(name) = line.strip_prefix("# branch.upstream ") {
            header.upstream = Some(name.to_owned());
        } else if let Some(counts) = line.strip_prefix("# branch.ab ") {
            let mut parts = counts.split_whitespace();
            let number = |part: Option<&str>, sign: char| {
                part.and_then(|text| text.strip_prefix(sign))
                    .and_then(|text| text.parse().ok())
                    .unwrap_or(0)
            };
            header.ahead = number(parts.next(), '+');
            header.behind = number(parts.next(), '-');
        }
    }
    header
}

/// The remote a push would go to: `origin`, else the first one listed.
fn pick_remote(remotes: &str) -> Option<String> {
    let names: Vec<&str> = remotes
        .lines()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .collect();
    names
        .iter()
        .find(|name| **name == "origin")
        .or(names.first())
        .map(|name| (*name).to_owned())
}

pub fn read(root: &Path) -> Result<BranchSync, AppError> {
    // No optional locks: an agent may be using the same worktree.
    let status = git(root)
        .args([
            "--no-optional-locks",
            "status",
            "--porcelain=v2",
            "--branch",
            "--untracked-files=no",
        ])
        .run()?;
    let header = parse_header(&status);
    let remote = pick_remote(&git(root).arg("remote").run()?);
    Ok(BranchSync {
        branch: header.branch,
        upstream: header.upstream,
        ahead: header.ahead,
        behind: header.behind,
        remote,
    })
}

/// Pushes the current branch. A branch with no upstream is published to the remote and set to
/// track it. Git's own refusal (a rejected non-fast-forward, a missing login) comes back as is.
pub fn push(root: &Path) -> Result<(), AppError> {
    let sync = read(root)?;
    let Some(branch) = sync.branch else {
        return Err(AppError::PushUnavailable("the branch is detached"));
    };
    if sync.upstream.is_some() {
        git(root).arg("push").run()?;
        return Ok(());
    }
    let Some(remote) = sync.remote else {
        return Err(AppError::PushUnavailable("there is no remote to push to"));
    };
    git(root)
        .args(["push", "--set-upstream", &remote, &branch])
        .run()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::test_repository::TestRepository;

    #[test]
    fn the_header_gives_branch_upstream_and_counts() {
        let header = parse_header(
            "# branch.oid abc\n# branch.head feat/x\n# branch.upstream origin/feat/x\n# branch.ab +2 -1\n",
        );

        assert_eq!(header.branch.as_deref(), Some("feat/x"));
        assert_eq!(header.upstream.as_deref(), Some("origin/feat/x"));
        assert_eq!((header.ahead, header.behind), (2, 1));
    }

    #[test]
    fn a_branch_with_no_upstream_or_a_detached_head_says_so() {
        let unpublished = parse_header("# branch.oid abc\n# branch.head main\n");
        assert_eq!(unpublished.upstream, None);
        assert_eq!(unpublished.ahead, 0);

        let detached = parse_header("# branch.oid abc\n# branch.head (detached)\n");
        assert_eq!(detached.branch, None);
    }

    #[test]
    fn origin_wins_among_remotes() {
        assert_eq!(pick_remote("backup\norigin\n").as_deref(), Some("origin"));
        assert_eq!(pick_remote("backup\nother\n").as_deref(), Some("backup"));
        assert_eq!(pick_remote(""), None);
    }

    #[test]
    fn a_repository_without_a_remote_cannot_push() {
        let repo = TestRepository::new();

        let sync = read(&repo.path).expect("read");
        assert_eq!(sync.remote, None);
        assert!(matches!(
            push(&repo.path),
            Err(AppError::PushUnavailable(_))
        ));
    }

    #[test]
    fn pushing_publishes_a_new_branch_and_then_sends_new_commits() {
        let repo = TestRepository::new().with_origin();
        // The helper pushes main without tracking it, so the branch counts as unpublished.
        assert_eq!(read(&repo.path).expect("read").upstream, None);

        push(&repo.path).expect("publish");
        let published = read(&repo.path).expect("read");
        assert_eq!(published.upstream.as_deref(), Some("origin/main"));
        assert_eq!(published.ahead, 0);

        repo.commit_file("a.txt", "a\n", "add a");
        assert_eq!(read(&repo.path).expect("read").ahead, 1);

        push(&repo.path).expect("push");
        assert_eq!(read(&repo.path).expect("read").ahead, 0);
    }
}
