//! The commits already made on the current branch, newest first, marked with whether the remote has them.

use std::collections::HashSet;
use std::path::Path;

use serde::Serialize;
use specta::Type;

use crate::error::AppError;
use crate::git::git_command::git;

use super::branch_sync;

const MAX_COMMITS: u32 = 200;
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

pub fn list(root: &Path, limit: u32) -> Result<Vec<CommitEntry>, AppError> {
    let limit = limit.clamp(1, MAX_COMMITS);
    let format = format!("--format=%H{SEP}%h{SEP}%an{SEP}%at{SEP}%s");
    let log = match git(root)
        .args(["log", "-n", &limit.to_string(), &format])
        .run()
    {
        Ok(text) => text,
        // A branch with no commit yet has nothing to list.
        Err(error) if error.stderr_contains("does not have any commits") => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut commits = parse_log(&log);

    let sync = branch_sync::read(root)?;
    let unpushed: HashSet<String> = match &sync.upstream {
        Some(upstream) => git(root)
            .args(["rev-list", &format!("{upstream}..HEAD")])
            .run()?
            .lines()
            .map(str::to_owned)
            .collect(),
        None => commits.iter().map(|commit| commit.hash.clone()).collect(),
    };
    for commit in &mut commits {
        commit.pushed = !unpushed.contains(&commit.hash);
    }
    Ok(commits)
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn the_limit_caps_the_list() {
        let repo = TestRepository::new();
        repo.commit_file("a.txt", "a\n", "add a");
        repo.commit_file("b.txt", "b\n", "add b");

        assert_eq!(list(&repo.path, 2).expect("list").len(), 2);
    }
}
