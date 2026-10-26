//! The changed files of a worktree, from `git status`.

use std::path::Path;
use std::thread;

use serde::Serialize;
use specta::Type;

use crate::git::GitError;

use super::line_counts::{LineCounts, attach as attach_line_counts, read_tracked};
use crate::git::git_command::git;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    TypeChanged,
    Untracked,
    Conflicted,
}

/// One path as Git reports it: what is staged and what is not, which can differ.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ChangedFile {
    /// Relative to the worktree, with `/`.
    pub path: String,
    /// Where a renamed or copied file came from.
    pub original_path: Option<String>,
    /// The change in the index (what the next commit would hold).
    pub staged: Option<ChangeKind>,
    /// The change in the working tree that is not staged yet. Untracked and conflicted files are here.
    pub unstaged: Option<ChangeKind>,
    /// Lines added and removed by the staged change. `None` for a binary file or a failed read.
    pub staged_lines: Option<LineCounts>,
    /// The same for the change that is not staged. An untracked file counts as all added.
    pub unstaged_lines: Option<LineCounts>,
}

fn kind_of(code: char) -> Option<ChangeKind> {
    match code {
        'A' => Some(ChangeKind::Added),
        'M' => Some(ChangeKind::Modified),
        'D' => Some(ChangeKind::Deleted),
        'R' => Some(ChangeKind::Renamed),
        'C' => Some(ChangeKind::Copied),
        'T' => Some(ChangeKind::TypeChanged),
        _ => None,
    }
}

fn is_conflict(index: char, worktree: char) -> bool {
    index == 'U'
        || worktree == 'U'
        || (index == 'A' && worktree == 'A')
        || (index == 'D' && worktree == 'D')
}

/// Reads `git status --porcelain=v1 -z`: `XY path` entries, with the source path after
/// a renamed or copied one.
pub fn parse_status(output: &str) -> Vec<ChangedFile> {
    let mut fields = output.split('\0').filter(|field| !field.is_empty());
    let mut files = Vec::new();
    while let Some(entry) = fields.next() {
        let mut codes = entry.chars();
        let (Some(index), Some(worktree)) = (codes.next(), codes.next()) else {
            continue;
        };
        let Some(path) = entry.get(3..) else { continue };
        let original_path = if matches!(index, 'R' | 'C') || matches!(worktree, 'R' | 'C') {
            fields.next().map(str::to_owned)
        } else {
            None
        };

        let (staged, unstaged) = if index == '?' && worktree == '?' {
            (None, Some(ChangeKind::Untracked))
        } else if index == '!' {
            continue;
        } else if is_conflict(index, worktree) {
            (None, Some(ChangeKind::Conflicted))
        } else {
            (kind_of(index), kind_of(worktree))
        };
        files.push(ChangedFile {
            path: path.to_owned(),
            original_path,
            staged,
            unstaged,
            staged_lines: None,
            unstaged_lines: None,
        });
    }
    files
}

pub fn list_changes(root: &Path) -> Result<Vec<ChangedFile>, GitError> {
    // The counts need no list of files, so they are read while `status` runs.
    let (output, counts) = thread::scope(|scope| {
        let counts = scope.spawn(|| read_tracked(root));
        // `--no-optional-locks`: a refresh must not fight an agent that is running git in the same worktree.
        let output = git(root)
            .args([
                "--no-optional-locks",
                "status",
                "--porcelain=v1",
                "-z",
                "--untracked-files=all",
            ])
            .run();
        (output, counts.join().unwrap_or_default())
    });
    let mut files = parse_status(&output?);
    attach_line_counts(root, &mut files, &counts);
    Ok(files)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::git::git_command::spawn_count;
    use crate::git::test_repository::TestRepository;

    #[test]
    fn a_refresh_of_the_changes_starts_three_git_processes_and_no_more() {
        let repo = TestRepository::new();
        fs::write(repo.path.join("new.txt"), "x\n").expect("write");
        let before = spawn_count::started_in(&repo.path);

        list_changes(&repo.path).expect("list");

        // The status and the two line counts, side by side.
        assert_eq!(spawn_count::started_in(&repo.path) - before, 3);
    }

    #[test]
    fn reads_staged_unstaged_and_untracked_entries() {
        let files = parse_status("M  a.rs\0 M b.rs\0MM c.rs\0A  d.rs\0 D e.rs\0?? f.rs\0");

        assert_eq!(files[0].staged, Some(ChangeKind::Modified));
        assert_eq!(files[0].unstaged, None);
        assert_eq!(files[1].staged, None);
        assert_eq!(files[1].unstaged, Some(ChangeKind::Modified));
        assert_eq!(files[2].staged, Some(ChangeKind::Modified));
        assert_eq!(files[2].unstaged, Some(ChangeKind::Modified));
        assert_eq!(files[3].staged, Some(ChangeKind::Added));
        assert_eq!(files[4].unstaged, Some(ChangeKind::Deleted));
        assert_eq!(files[5].unstaged, Some(ChangeKind::Untracked));
        assert_eq!(files[5].path, "f.rs");
    }

    #[test]
    fn a_rename_carries_the_path_it_came_from() {
        let files = parse_status("R  new name.rs\0old name.rs\0 M other.rs\0");

        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "new name.rs");
        assert_eq!(files[0].original_path.as_deref(), Some("old name.rs"));
        assert_eq!(files[0].staged, Some(ChangeKind::Renamed));
        assert_eq!(files[1].path, "other.rs");
    }

    #[test]
    fn conflicts_are_one_kind_whatever_the_codes() {
        let files = parse_status("UU a.rs\0AA b.rs\0DD c.rs\0AU d.rs\0");

        assert!(files.iter().all(|file| file.staged.is_none()));
        assert!(
            files
                .iter()
                .all(|file| file.unstaged == Some(ChangeKind::Conflicted))
        );
    }

    #[test]
    fn ignored_entries_are_left_out() {
        assert!(parse_status("!! target/\0").is_empty());
    }

    #[test]
    fn lists_the_changes_of_a_real_repository() {
        let repo = TestRepository::new();
        repo.commit_file("keep.txt", "one\n", "add keep");
        fs::write(repo.path.join("keep.txt"), "two\n").expect("edit");
        fs::create_dir(repo.path.join("dir")).expect("mkdir");
        fs::write(repo.path.join("dir/new.txt"), "x\n").expect("write");
        fs::write(repo.path.join("staged.txt"), "y\n").expect("write");
        repo.git(&["add", "staged.txt"]);

        let files = list_changes(&repo.path).expect("status");
        let of = |path: &str| files.iter().find(|file| file.path == path).expect(path);

        assert_eq!(of("keep.txt").unstaged, Some(ChangeKind::Modified));
        assert_eq!(of("staged.txt").staged, Some(ChangeKind::Added));
        // Every file of an untracked folder is listed, not just the folder.
        assert_eq!(of("dir/new.txt").unstaged, Some(ChangeKind::Untracked));
    }

    #[test]
    fn a_clean_worktree_has_no_changes() {
        let repo = TestRepository::new();
        assert!(list_changes(&repo.path).expect("status").is_empty());
    }
}
