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

struct Position {
    branch: Option<String>,
    upstream: Option<String>,
    ahead: u32,
    behind: u32,
}

/// Reads `ahead 2, behind 1` (what `%(upstream:track,nobracket)` prints); `gone` and nothing at
/// all both mean no distance to report.
fn parse_track(track: &str) -> (u32, u32) {
    let mut counts = (0, 0);
    for part in track.split(',').map(str::trim) {
        if let Some(number) = part.strip_prefix("ahead ") {
            counts.0 = number.parse().unwrap_or(0);
        } else if let Some(number) = part.strip_prefix("behind ") {
            counts.1 = number.parse().unwrap_or(0);
        }
    }
    counts
}

/// Where HEAD stands against its upstream, from the refs alone. `git status --branch` says the
/// same, but only after it has scanned the whole worktree, which is what a big repository pays for.
///
/// One `for-each-ref` over the local branches gives the current one (`%(HEAD)`), its upstream and
/// the distance. A branch with no commit yet has no ref, and a detached HEAD is on none; only then
/// is a second process asked which of the two it is.
fn read_position(root: &Path) -> Result<Position, AppError> {
    let refs = git(root)
        .args([
            "for-each-ref",
            "--format=%(HEAD)%09%(refname)%09%(upstream:short)%09%(upstream:track,nobracket)",
            "refs/heads/",
        ])
        .run()?;
    for line in refs.lines() {
        let mut fields = line.split('\t');
        if fields.next().map(str::trim) != Some("*") {
            continue;
        }
        let branch = fields
            .next()
            .and_then(|name| name.strip_prefix("refs/heads/"))
            .map(str::to_owned);
        let upstream = fields
            .next()
            .filter(|name| !name.is_empty())
            .map(str::to_owned);
        let (ahead, behind) = parse_track(fields.next().unwrap_or(""));
        return Ok(Position {
            branch,
            upstream,
            ahead,
            behind,
        });
    }
    // Fails on a detached HEAD, which is an answer, not a problem.
    let unborn = git(root)
        .args(["symbolic-ref", "-q", "--short", "HEAD"])
        .run()
        .ok()
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty());
    Ok(Position {
        branch: unborn,
        upstream: None,
        ahead: 0,
        behind: 0,
    })
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
    // Neither needs the other, so both run at once.
    let (header, remotes) = std::thread::scope(|scope| {
        let remotes = scope.spawn(|| git(root).arg("remote").run());
        (read_position(root), remotes.join())
    });
    let header = header?;
    let remotes = remotes.map_err(|_| AppError::TaskFailed("reading the remotes".to_owned()))??;
    let remote = pick_remote(&remotes);
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
    use crate::git::git_command::spawn_count;
    use crate::git::test_repository::{TestRepository, set_identity};

    #[test]
    fn the_track_gives_the_distance_in_both_directions() {
        assert_eq!(parse_track("ahead 2, behind 1"), (2, 1));
        assert_eq!(parse_track("ahead 3"), (3, 0));
        assert_eq!(parse_track("behind 4"), (0, 4));
        assert_eq!(parse_track("gone"), (0, 0));
        assert_eq!(parse_track(""), (0, 0));
    }

    #[test]
    fn a_branch_with_no_upstream_a_gone_upstream_or_a_detached_head_says_so() {
        let repo = TestRepository::new().with_origin();
        assert_eq!(read(&repo.path).expect("read").upstream, None);

        publish(&repo);
        let published = read(&repo.path).expect("read");
        assert_eq!(published.upstream.as_deref(), Some("origin/main"));
        assert_eq!((published.ahead, published.behind), (0, 0));

        repo.commit_file("later.md", "x\n", "later");
        assert_eq!(read(&repo.path).expect("read").ahead, 1);

        // The remote branch disappears: the upstream is still configured, with nothing to compare.
        repo.git(&["update-ref", "-d", "refs/remotes/origin/main"]);
        let gone = read(&repo.path).expect("read");
        assert_eq!(gone.upstream.as_deref(), Some("origin/main"));
        assert_eq!((gone.ahead, gone.behind), (0, 0));

        repo.git(&["checkout", "--quiet", "--detach"]);
        assert_eq!(read(&repo.path).expect("read").branch, None);
    }

    #[test]
    fn an_unborn_branch_is_not_mistaken_for_one_that_starts_with_its_name() {
        let repo = TestRepository::new().with_origin();
        repo.git(&["checkout", "--quiet", "-b", "feat/x"]);
        repo.git(&["push", "--quiet", "--set-upstream", "origin", "feat/x"]);
        // `feat` has no commit, so no ref: a lookup by pattern would find `feat/x` instead.
        repo.git(&["checkout", "--quiet", "--orphan", "feat"]);

        let sync = read(&repo.path).expect("read");
        assert_eq!(sync.branch.as_deref(), Some("feat"));
        assert_eq!(sync.upstream, None);
    }

    #[test]
    fn a_branch_with_no_commit_yet_has_a_name_and_nothing_to_compare() {
        let dir = tempfile::tempdir().expect("dir");
        git(dir.path())
            .args(["init", "--quiet", "-b", "main"])
            .run()
            .expect("init");

        let sync = read(dir.path()).expect("read");
        assert_eq!(sync.branch.as_deref(), Some("main"));
        assert_eq!(sync.upstream, None);
    }

    #[test]
    fn reading_the_branch_starts_two_git_processes_and_no_more() {
        let repo = TestRepository::new().with_origin();
        publish(&repo);
        let before = spawn_count::started_in(&repo.path);

        read(&repo.path).expect("read");

        // The branches with their upstream, and the remotes.
        assert_eq!(spawn_count::started_in(&repo.path) - before, 2);
    }

    #[test]
    fn a_branch_among_many_is_found_by_the_star_that_marks_it() {
        let repo = TestRepository::new().with_origin();
        publish(&repo);
        repo.git(&["branch", "other"]);
        repo.git(&["branch", "main-two"]);
        repo.git(&["checkout", "--quiet", "other"]);

        let sync = read(&repo.path).expect("read");

        assert_eq!(sync.branch.as_deref(), Some("other"));
        assert_eq!(sync.upstream, None);
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
        set_identity(&other);
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
