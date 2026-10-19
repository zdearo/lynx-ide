//! Lines added and removed per changed file, from `git diff --numstat`. Decoration for the list:
//! a failed read just leaves the counts out.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::Serialize;
use specta::Type;

use crate::git::git_command::git;

use super::change_list::{ChangeKind, ChangedFile};

/// Reading every new file on each refresh would be slow in a worktree full of them.
const MAX_UNTRACKED_COUNTED: usize = 200;
const MAX_UNTRACKED_BYTES: u64 = 256 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LineCounts {
    pub added: u32,
    pub removed: u32,
}

/// Reads `git diff --numstat -z -M`. A renamed file is keyed by its new path, and binary files
/// (`-` counts) are left out.
pub fn parse_numstat(output: &str) -> HashMap<String, LineCounts> {
    let mut counts = HashMap::new();
    let mut tokens = output.split('\0');
    while let Some(token) = tokens.next() {
        let mut parts = token.splitn(3, '\t');
        let (Some(added), Some(removed), Some(path)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        // A rename has an empty path here, and the old and new paths follow as two more fields.
        let path = if path.is_empty() {
            tokens.next();
            tokens.next().unwrap_or("")
        } else {
            path
        };
        if let (Ok(added), Ok(removed)) = (added.parse::<u32>(), removed.parse::<u32>())
            && !path.is_empty()
        {
            counts.insert(path.to_owned(), LineCounts { added, removed });
        }
    }
    counts
}

/// The lines of a text file, or `None` for a binary one.
pub fn count_text_lines(bytes: &[u8]) -> Option<u32> {
    if bytes.contains(&0) {
        return None;
    }
    let newlines = bytes.iter().filter(|byte| **byte == b'\n').count();
    let unterminated = usize::from(bytes.last().is_some_and(|byte| *byte != b'\n'));
    u32::try_from(newlines + unterminated).ok()
}

fn numstat(root: &Path, cached: bool) -> HashMap<String, LineCounts> {
    let mut command = git(root).args(["--no-optional-locks", "diff", "--numstat", "-z", "-M"]);
    if cached {
        command = command.arg("--cached");
    }
    command
        .run_bytes()
        .map(|bytes| parse_numstat(&String::from_utf8_lossy(&bytes)))
        .unwrap_or_default()
}

/// Fills `staged_lines` and `unstaged_lines`. An untracked file counts as all added.
pub fn attach(root: &Path, files: &mut [ChangedFile]) {
    let staged = numstat(root, true);
    let unstaged = numstat(root, false);
    let mut untracked_read = 0;
    for file in files {
        file.staged_lines = staged.get(&file.path).copied();
        file.unstaged_lines = unstaged.get(&file.path).copied();
        if file.unstaged == Some(ChangeKind::Untracked) && untracked_read < MAX_UNTRACKED_COUNTED {
            untracked_read += 1;
            file.unstaged_lines = untracked_counts(&root.join(&file.path));
        }
    }
}

fn untracked_counts(path: &Path) -> Option<LineCounts> {
    let size = fs::metadata(path).ok()?.len();
    if size > MAX_UNTRACKED_BYTES {
        return None;
    }
    let added = count_text_lines(&fs::read(path).ok()?)?;
    Some(LineCounts { added, removed: 0 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numstat_reads_counts_by_path_and_skips_binary_files() {
        let output = "3\t1\tsrc/a.rs\0-\t-\timage.png\0";

        let counts = parse_numstat(output);

        assert_eq!(
            counts.get("src/a.rs"),
            Some(&LineCounts {
                added: 3,
                removed: 1
            })
        );
        assert!(!counts.contains_key("image.png"));
    }

    #[test]
    fn a_rename_is_keyed_by_its_new_path() {
        let output = "2\t0\t\0old/name.rs\0new/name.rs\0";

        let counts = parse_numstat(output);

        assert_eq!(
            counts.get("new/name.rs"),
            Some(&LineCounts {
                added: 2,
                removed: 0
            })
        );
        assert!(!counts.contains_key("old/name.rs"));
    }

    #[test]
    fn lines_are_counted_with_or_without_a_final_newline() {
        assert_eq!(count_text_lines(b"a\nb\n"), Some(2));
        assert_eq!(count_text_lines(b"a\nb"), Some(2));
        assert_eq!(count_text_lines(b""), Some(0));
        assert_eq!(count_text_lines(b"bin\0ary"), None);
    }

    #[test]
    fn a_real_repository_gives_counts_for_unstaged_staged_and_new_files() {
        use crate::changes::change_list::list_changes;
        use crate::git::test_repository::TestRepository;
        let repo = TestRepository::new();
        fs::write(repo.path.join("README.md"), "hello\nmore\nlines\n").expect("write");
        fs::write(repo.path.join("new.txt"), "a\nb\nc\n").expect("write");

        let files = list_changes(&repo.path).expect("list");
        let readme = files
            .iter()
            .find(|f| f.path == "README.md")
            .expect("readme");
        let new = files.iter().find(|f| f.path == "new.txt").expect("new");
        assert_eq!(
            readme.unstaged_lines,
            Some(LineCounts {
                added: 2,
                removed: 0
            })
        );
        assert_eq!(
            new.unstaged_lines,
            Some(LineCounts {
                added: 3,
                removed: 0
            })
        );

        repo.git(&["add", "README.md"]);
        let files = list_changes(&repo.path).expect("list");
        let readme = files
            .iter()
            .find(|f| f.path == "README.md")
            .expect("readme");
        assert_eq!(
            readme.staged_lines,
            Some(LineCounts {
                added: 2,
                removed: 0
            })
        );
        assert_eq!(readme.unstaged_lines, None);
    }
}
