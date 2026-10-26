//! The commits already made on the current branch, newest first, marked with whether the remote has them.

use std::collections::HashSet;
use std::path::Path;
use std::thread;

use serde::Serialize;
use specta::Type;

use crate::error::AppError;
use crate::git::git_command::git;

use super::change_list::ChangeKind;
use super::file_diff::is_commit_hash;
use super::line_counts::{LineCounts, parse_numstat};

const MAX_COMMITS: u32 = 200;
/// How far back to look for commits the remote lacks; the list never shows more than `MAX_COMMITS`.
const UNPUSHED_LOOKBACK: u32 = 500;
/// Unit separator: git prints it between fields, and no subject or name can contain it.
const SEP: char = '\u{1f}';

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CommitEntry {
    pub hash: String,
    pub short_hash: String,
    pub subject: String,
    pub author: String,
    /// Seconds since 1970, as git records them.
    pub timestamp: u32,
    /// `false` for a commit the branch's upstream does not have. Without an upstream, nothing is
    /// known about the remote, so every commit reads as not pushed.
    pub pushed: bool,
}

fn parse_log(text: &str) -> Vec<CommitEntry> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.splitn(5, SEP);
            let hash = fields.next()?.to_owned();
            let short_hash = fields.next()?.to_owned();
            let author = fields.next()?.to_owned();
            let timestamp = fields.next()?.parse::<u32>().unwrap_or(0);
            let subject = fields.next()?.to_owned();
            Some(CommitEntry {
                hash,
                short_hash,
                subject,
                author,
                timestamp,
                pushed: true,
            })
        })
        .collect()
}

/// One file a commit touched, with the lines it added and removed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CommitFile {
    pub path: String,
    /// Where a renamed or copied file came from.
    pub original_path: Option<String>,
    pub kind: ChangeKind,
    pub lines: Option<LineCounts>,
}

/// Reads `--name-status -z`: a letter, then one path, or two for a rename or a copy.
fn parse_name_status(output: &str) -> Vec<(ChangeKind, Option<String>, String)> {
    let mut files = Vec::new();
    let mut tokens = output.split('\0').filter(|token| !token.is_empty());
    while let Some(status) = tokens.next() {
        let kind = match status.chars().next() {
            Some('A') => ChangeKind::Added,
            Some('M') => ChangeKind::Modified,
            Some('D') => ChangeKind::Deleted,
            Some('R') => ChangeKind::Renamed,
            Some('C') => ChangeKind::Copied,
            Some('T') => ChangeKind::TypeChanged,
            _ => continue,
        };
        let moved = matches!(kind, ChangeKind::Renamed | ChangeKind::Copied);
        let Some(first) = tokens.next() else { break };
        if moved {
            let Some(second) = tokens.next() else { break };
            files.push((kind, Some(first.to_owned()), second.to_owned()));
        } else {
            files.push((kind, None, first.to_owned()));
        }
    }
    files
}

/// The files of one commit against its first parent, in the order git lists them.
pub fn list_files(root: &Path, hash: &str) -> Result<Vec<CommitFile>, AppError> {
    if !is_commit_hash(hash) {
        return Err(AppError::ChangeRefused("that is not a commit hash"));
    }
    let show = |flag: &str| {
        git(root)
            .args([
                "show",
                "--format=",
                "--first-parent",
                "-M",
                "-z",
                flag,
                hash,
            ])
            .run_bytes()
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
    };
    let statuses = parse_name_status(&show("--name-status")?);
    let counts = parse_numstat(&show("--numstat")?);
    Ok(statuses
        .into_iter()
        .map(|(kind, original_path, path)| CommitFile {
            lines: counts.get(&path).copied(),
            path,
            original_path,
            kind,
        })
        .collect())
}

pub fn list(root: &Path, limit: u32) -> Result<Vec<CommitEntry>, AppError> {
    let limit = limit.clamp(1, MAX_COMMITS);
    let format = format!("--format=%H{SEP}%h{SEP}%an{SEP}%at{SEP}%s");
    // The two reads are independent: git resolves `@{upstream}` itself, so the branch name is not
    // needed first. Only the newest commits are listed, so only that many unpushed ones can matter.
    let (log, unpushed) = thread::scope(|scope| {
        let unpushed = scope.spawn(|| {
            git(root)
                .args([
                    "rev-list",
                    "-n",
                    &UNPUSHED_LOOKBACK.to_string(),
                    "HEAD",
                    "^@{upstream}",
                ])
                .run()
        });
        let log = git(root)
            .args(["log", "-n", &limit.to_string(), &format])
            .run();
        (log, unpushed.join())
    });
    let log = match log {
        Ok(text) => text,
        // A branch with no commit yet has nothing to list.
        Err(error) if error.stderr_contains("does not have any commits") => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut commits = parse_log(&log);

    let unpushed: HashSet<String> = match unpushed {
        Ok(Ok(text)) => text.lines().map(str::to_owned).collect(),
        // No upstream, or one whose branch is gone from the remote: nothing is known to be there.
        _ => commits.iter().map(|commit| commit.hash.clone()).collect(),
    };
    for commit in &mut commits {
        commit.pushed = !unpushed.contains(&commit.hash);
    }
    Ok(commits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::changes::branch_sync;
    use crate::git::git_command::spawn_count;
    use crate::git::test_repository::TestRepository;

    #[test]
    fn lines_split_into_fields_and_a_subject_keeps_its_separators() {
        let line = format!("abc{SEP}ab{SEP}Ana{SEP}1700000000{SEP}fix: a | b, c");

        let commits = parse_log(&line);

        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].short_hash, "ab");
        assert_eq!(commits[0].timestamp, 1_700_000_000);
        assert_eq!(commits[0].subject, "fix: a | b, c");
    }

    #[test]
    fn a_repository_without_commits_lists_nothing() {
        let dir = tempfile::tempdir().expect("temp dir");
        git(dir.path()).args(["init", "-q"]).run().expect("init");

        assert!(list(dir.path(), 10).expect("list").is_empty());
    }

    #[test]
    fn commits_come_newest_first_and_the_ones_the_remote_lacks_are_marked() {
        let repo = TestRepository::new().with_origin();
        branch_sync::push(&repo.path).expect("publish");
        repo.commit_file("a.txt", "a\n", "add a");

        let commits = list(&repo.path, 10).expect("list");

        let subjects: Vec<_> = commits.iter().map(|c| c.subject.as_str()).collect();
        assert_eq!(subjects, ["add a", "initial commit"]);
        assert!(!commits[0].pushed);
        assert!(commits[1].pushed);
    }

    #[test]
    fn without_an_upstream_no_commit_counts_as_pushed() {
        let repo = TestRepository::new();

        let commits = list(&repo.path, 10).expect("list");

        assert!(commits.iter().all(|commit| !commit.pushed));
    }

    #[test]
    fn an_upstream_gone_from_the_remote_still_lists_the_commits_as_not_pushed() {
        let repo = TestRepository::new().with_origin();
        branch_sync::push(&repo.path).expect("publish");
        repo.git(&["update-ref", "-d", "refs/remotes/origin/main"]);

        let commits = list(&repo.path, 10).expect("list");

        assert_eq!(commits.len(), 1);
        assert!(!commits[0].pushed);
    }

    #[test]
    fn listing_starts_two_git_processes_and_no_more() {
        let repo = TestRepository::new().with_origin();
        branch_sync::push(&repo.path).expect("publish");
        let before = spawn_count::started_in(&repo.path);

        list(&repo.path, 10).expect("list");

        // The log and the commits the remote lacks; the branch is not read again.
        assert_eq!(spawn_count::started_in(&repo.path) - before, 2);
    }

    #[test]
    fn the_limit_caps_the_list() {
        let repo = TestRepository::new();
        repo.commit_file("a.txt", "a\n", "add a");
        repo.commit_file("b.txt", "b\n", "add b");

        assert_eq!(list(&repo.path, 2).expect("list").len(), 2);
    }

    #[test]
    fn name_status_reads_letters_paths_and_renames() {
        let output = "M\0src/a.rs\0R100\0old.rs\0new.rs\0D\0gone.rs\0";

        let files = parse_name_status(output);

        assert_eq!(files.len(), 3);
        assert_eq!(
            files[0],
            (ChangeKind::Modified, None, "src/a.rs".to_owned())
        );
        assert_eq!(
            files[1],
            (
                ChangeKind::Renamed,
                Some("old.rs".to_owned()),
                "new.rs".to_owned()
            )
        );
        assert_eq!(files[2].0, ChangeKind::Deleted);
    }

    #[test]
    fn a_commit_lists_its_files_with_kinds_and_counts_and_a_root_lists_what_it_added() {
        let repo = TestRepository::new();
        repo.commit_file("README.md", "hello\nworld\n", "edit");
        repo.commit_file("new.txt", "a\nb\n", "add");
        let edit = repo.git(&["rev-parse", "HEAD~1"]);
        let root_commit = repo.git(&["rev-parse", "HEAD~2"]);

        let files = list_files(&repo.path, &edit).expect("files");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "README.md");
        assert_eq!(files[0].kind, ChangeKind::Modified);
        assert_eq!(
            files[0].lines,
            Some(LineCounts {
                added: 1,
                removed: 0
            })
        );

        let root = list_files(&repo.path, &root_commit).expect("root");
        assert_eq!(root[0].kind, ChangeKind::Added);
        assert!(list_files(&repo.path, "--oops").is_err());
    }
}
