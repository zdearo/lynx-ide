//! Which folders of a worktree are worth a watch.

use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

/// One watch per folder, and the system caps them (inotify's default is in the thousands, shared
/// with everything else the user runs). A worktree past this is read on a timer instead.
pub const MAX_WATCHED_DIRECTORIES: usize = 4000;

/// `start` and every folder below it that Git does not ignore, so `node_modules` and `target`
/// never cost a watch. `None` when there are more than `limit`.
pub fn directories_below(start: &Path, limit: usize) -> Option<Vec<PathBuf>> {
    let walk = WalkBuilder::new(start)
        .hidden(false)
        // A linked worktree's `.git` is a file; the ignore rules still apply without a repository.
        .require_git(false)
        .filter_entry(|entry| entry.file_name() != ".git")
        .build();

    let mut directories = Vec::new();
    for entry in walk.flatten() {
        if entry.file_type().is_some_and(|kind| kind.is_dir()) {
            if directories.len() >= limit {
                return None;
            }
            directories.push(entry.into_path());
        }
    }
    Some(directories)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn tree() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("dir");
        for folder in ["src/ui", "node_modules/pkg/lib", ".git/objects", ".github"] {
            fs::create_dir_all(dir.path().join(folder)).expect("create");
        }
        fs::write(dir.path().join(".gitignore"), "node_modules/\n").expect("ignore file");
        dir
    }

    fn relative(dir: &Path, found: Vec<PathBuf>) -> Vec<String> {
        let mut names: Vec<String> = found
            .iter()
            .map(|path| {
                path.strip_prefix(dir)
                    .expect("inside")
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();
        names.sort();
        names
    }

    #[test]
    fn ignored_folders_and_dot_git_cost_no_watch_but_other_hidden_folders_do() {
        let dir = tree();
        let found = directories_below(dir.path(), 100).expect("small");
        assert_eq!(
            relative(dir.path(), found),
            ["", ".github", "src", "src/ui"]
        );
    }

    #[test]
    fn a_tree_over_the_limit_is_refused() {
        let dir = tree();
        assert!(directories_below(dir.path(), 3).is_none());
        assert!(directories_below(dir.path(), 4).is_some());
    }
}
