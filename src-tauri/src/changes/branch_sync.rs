//! Where the current branch stands against its remote, and pushing it. Never a force push.

use std::path::Path;
use std::time::Duration;

use serde::Serialize;
use specta::Type;

use crate::error::AppError;
use crate::git::git_command::git;

/// Pushing, pulling and fetching wait on the network, so they get a limit.
pub const NETWORK_TIMEOUT: Duration = Duration::from_secs(180);

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
    /// The branch is both ahead and behind, and every commit only the remote has is an older copy
    /// of one that was rewritten here (an amend, a rebase). A plain push is refused then; a force push
    /// with lease is the way, and a pull would only bring the old copies back.
    pub rewritten: bool,
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
    let rewritten = header.ahead > 0
        && header.behind > 0
        && header
            .upstream
            .as_deref()
            .is_some_and(|upstream| remote_only_has_old_copies(root, upstream));
    Ok(BranchSync {
        branch: header.branch,
        upstream: header.upstream,
        ahead: header.ahead,
        behind: header.behind,
        remote,
        rewritten,
    })
}

/// Every commit only the upstream has appears in this branch's own reflog: it was here once, and
/// was replaced. A commit someone else pushed never did.
fn remote_only_has_old_copies(root: &Path, upstream: &str) -> bool {
    let Ok(only_remote) = git(root)
        .args(["rev-list", &format!("HEAD..{upstream}")])
        .run()
    else {
        return false;
    };
    let Ok(reflog) = git(root)
        .args(["reflog", "show", "--format=%H", "-n", "300", "HEAD"])
        .run()
    else {
        return false;
    };
    let known: std::collections::HashSet<&str> = reflog.lines().collect();
    let mut commits = only_remote.lines().peekable();
    commits.peek().is_some() && commits.all(|hash| known.contains(hash))
}

/// Reads the remote's news. Refs only: the working tree and the branch are not touched.
pub fn fetch(root: &Path) -> Result<(), AppError> {
    let Some(remote) = read(root)?.remote else {
        return Ok(());
    };
    git(root)
        .args(["fetch", "--quiet", &remote])
        .timeout(NETWORK_TIMEOUT)
        .run()?;
    Ok(())
}

/// How a pull joins the remote's commits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum PullKind {
    /// Only moves forward: refused when the branch has commits of its own.
    FastForward,
    /// Puts the branch's own commits on top of the remote's. A conflict stops it, and it is undone.
    Rebase,
}

/// Brings the upstream's commits into the branch. A rebase that hits a conflict is aborted, so the
/// repository is never left in the middle of one: the conflict screen is not part of Lynx yet.
pub fn pull(root: &Path, kind: PullKind) -> Result<(), AppError> {
    let sync = read(root)?;
    if sync.upstream.is_none() {
        return Err(AppError::PushUnavailable(
            "the branch has no upstream to pull from",
        ));
    }
    let flag = match kind {
        PullKind::FastForward => "--ff-only",
        PullKind::Rebase => "--rebase",
    };
    let result = git(root)
        .args(["pull", flag])
        .timeout(NETWORK_TIMEOUT)
        .run();
    if let Err(error) = result {
        if kind == PullKind::Rebase {
            let _ = git(root).args(["rebase", "--abort"]).run();
        }
        return Err(error.into());
    }
    Ok(())
}

/// Replaces the remote's copy of the branch with this one, but only if the remote still holds what
/// was last fetched: someone else's push in between makes git refuse.
pub fn force_push_with_lease(root: &Path) -> Result<(), AppError> {
    let sync = read(root)?;
    if sync.branch.is_none() {
        return Err(AppError::PushUnavailable("the branch is detached"));
    }
    if sync.upstream.is_none() {
        return Err(AppError::PushUnavailable(
            "the branch has no upstream to replace",
        ));
    }
    git(root)
        .args(["push", "--force-with-lease"])
        .timeout(NETWORK_TIMEOUT)
        .run()?;
    Ok(())
}

/// Pushes the current branch. A branch with no upstream is published to the remote and set to
/// track it. Git's own refusal (a rejected non-fast-forward, a missing login) comes back as is.
pub fn push(root: &Path) -> Result<(), AppError> {
    let sync = read(root)?;
    let Some(branch) = sync.branch else {
        return Err(AppError::PushUnavailable("the branch is detached"));
    };
    if sync.upstream.is_some() {
        git(root).arg("push").timeout(NETWORK_TIMEOUT).run()?;
        return Ok(());
    }
    let Some(remote) = sync.remote else {
        return Err(AppError::PushUnavailable("there is no remote to push to"));
    };
    git(root)
        .args(["push", "--set-upstream", &remote, &branch])
        .timeout(NETWORK_TIMEOUT)
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

    /// A second clone of the origin, where "someone else" works.
    fn other_clone(repo: &TestRepository) -> std::path::PathBuf {
        let origin = repo.path.with_file_name("origin.git");
        let other = repo.path.with_file_name("other");
        git(repo.path.parent().expect("parent"))
            .args(["clone", "-q", &origin.to_string_lossy(), "other"])
            .run()
            .expect("clone");
        git(&other)
            .args(["checkout", "-q", "-b", "main", "origin/main"])
            .run()
            .expect("checkout");
        other
    }

    fn publish(repo: &TestRepository) {
        push(&repo.path).expect("publish");
    }

    #[test]
    fn a_branch_behind_its_remote_fast_forwards() {
        let repo = TestRepository::new().with_origin();
        publish(&repo);
        let other = other_clone(&repo);
        crate::git::test_repository::commit_file_in(&other, "theirs.txt", "t\n", "theirs");
        git(&other)
            .args(["push", "-q", "origin", "main"])
            .run()
            .expect("push");

        fetch(&repo.path).expect("fetch");
        assert_eq!(read(&repo.path).expect("read").behind, 1);

        pull(&repo.path, PullKind::FastForward).expect("pull");
        let sync = read(&repo.path).expect("read");
        assert_eq!((sync.ahead, sync.behind), (0, 0));
        assert!(repo.path.join("theirs.txt").exists());
    }

    #[test]
    fn a_diverged_branch_rebases_onto_new_remote_commits_and_is_not_called_rewritten() {
        let repo = TestRepository::new().with_origin();
        publish(&repo);
        let other = other_clone(&repo);
        crate::git::test_repository::commit_file_in(&other, "theirs.txt", "t\n", "theirs");
        git(&other)
            .args(["push", "-q", "origin", "main"])
            .run()
            .expect("push");
        repo.commit_file("mine.txt", "m\n", "mine");
        fetch(&repo.path).expect("fetch");

        let before = read(&repo.path).expect("read");
        assert_eq!((before.ahead, before.behind), (1, 1));
        assert!(
            !before.rewritten,
            "a teammate's commit is not an old copy of mine"
        );

        assert!(pull(&repo.path, PullKind::FastForward).is_err());
        pull(&repo.path, PullKind::Rebase).expect("rebase");
        let after = read(&repo.path).expect("read");
        assert_eq!((after.ahead, after.behind), (1, 0));
    }

    #[test]
    fn a_rebase_that_conflicts_is_undone_and_leaves_no_rebase_behind() {
        let repo = TestRepository::new().with_origin();
        publish(&repo);
        let other = other_clone(&repo);
        crate::git::test_repository::commit_file_in(&other, "README.md", "theirs\n", "theirs");
        git(&other)
            .args(["push", "-q", "origin", "main"])
            .run()
            .expect("push");
        repo.commit_file("README.md", "mine\n", "mine");
        fetch(&repo.path).expect("fetch");

        assert!(pull(&repo.path, PullKind::Rebase).is_err());

        assert!(!repo.path.join(".git/rebase-merge").exists());
        assert_eq!(
            std::fs::read_to_string(repo.path.join("README.md")).expect("read"),
            "mine\n"
        );
    }

    #[test]
    fn an_amended_pushed_commit_reads_as_rewritten_and_a_force_push_with_lease_replaces_it() {
        let repo = TestRepository::new().with_origin();
        publish(&repo);
        repo.git(&[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--amend",
            "-q",
            "-m",
            "rewritten",
        ]);

        let sync = read(&repo.path).expect("read");
        assert_eq!((sync.ahead, sync.behind), (1, 1));
        assert!(sync.rewritten);
        assert!(push(&repo.path).is_err(), "a plain push is refused");

        force_push_with_lease(&repo.path).expect("force with lease");
        let after = read(&repo.path).expect("read");
        assert_eq!((after.ahead, after.behind), (0, 0));
        assert!(!after.rewritten);
    }
}
