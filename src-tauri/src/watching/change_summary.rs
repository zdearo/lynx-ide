//! Boils a burst of file events down to what the interface needs to read again.

use std::collections::BTreeSet;
use std::path::{Component, Path};

use super::worktree_events::WorktreeChanged;

/// More folders than this and the list stops being useful: the screen reads everything again.
const MAX_LISTED_DIRECTORIES: usize = 200;

/// Git files whose change means the staged files or the branch moved.
const GIT_STATE_FILES: [&str; 2] = ["index", "HEAD"];

#[derive(Debug, Default)]
pub struct ChangeSummary {
    directories: BTreeSet<String>,
    git: bool,
}

impl ChangeSummary {
    /// Counts one changed path. `git_dir` is where this worktree keeps its Git state, which is
    /// outside `root` for a linked worktree; anything else under `.git` is noise.
    pub fn add(&mut self, root: &Path, git_dir: Option<&Path>, path: &Path) {
        if let Some(git_dir) = git_dir
            && let Ok(inside) = path.strip_prefix(git_dir)
        {
            let name = inside.to_str().unwrap_or_default();
            if GIT_STATE_FILES.contains(&name) {
                self.git = true;
            }
            return;
        }
        let Ok(relative) = path.strip_prefix(root) else {
            return;
        };
        if relative.components().any(|part| part.as_os_str() == ".git") {
            return;
        }
        // The folder that holds the changed entry is the listing that is now out of date.
        let parent = relative.parent().unwrap_or(Path::new(""));
        self.directories.insert(slash_separated(parent));
    }

    pub fn is_empty(&self) -> bool {
        self.directories.is_empty() && !self.git
    }

    pub fn into_event(self, worktree_path: &str) -> WorktreeChanged {
        let everything = self.directories.len() > MAX_LISTED_DIRECTORIES;
        WorktreeChanged {
            worktree_path: worktree_path.to_owned(),
            directories: if everything {
                Vec::new()
            } else {
                self.directories.into_iter().collect()
            },
            everything,
            git: self.git,
        }
    }
}

/// `src/ui` on every platform, since the interface speaks in slashes.
fn slash_separated(path: &Path) -> String {
    path.components()
        .filter_map(|part| match part {
            Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn root() -> PathBuf {
        PathBuf::from("/work/app")
    }

    #[test]
    fn a_changed_file_marks_its_folder_and_the_root_is_the_empty_string() {
        let mut summary = ChangeSummary::default();
        summary.add(&root(), None, &root().join("README.md"));
        summary.add(&root(), None, &root().join("src/ui/Btn.tsx"));
        summary.add(&root(), None, &root().join("src/ui/Card.tsx"));

        let event = summary.into_event("/work/app");
        assert_eq!(event.directories, ["", "src/ui"]);
        assert!(!event.git && !event.everything);
    }

    #[test]
    fn git_internals_are_noise_except_the_index_and_head() {
        let git_dir = root().join(".git");
        let mut summary = ChangeSummary::default();
        summary.add(&root(), Some(&git_dir), &git_dir.join("objects/ab/cdef"));
        summary.add(&root(), Some(&git_dir), &git_dir.join("index.lock"));
        assert!(summary.is_empty());

        summary.add(&root(), Some(&git_dir), &git_dir.join("index"));
        assert!(summary.into_event("/work/app").git);
    }

    #[test]
    fn a_linked_worktrees_git_state_lives_outside_its_folder() {
        let git_dir = PathBuf::from("/work/app/.git/worktrees/task");
        let linked = PathBuf::from("/work/app-task");
        let mut summary = ChangeSummary::default();
        summary.add(&linked, Some(&git_dir), &git_dir.join("HEAD"));
        summary.add(&linked, Some(&git_dir), &git_dir.join("gitdir"));

        let event = summary.into_event("/work/app-task");
        assert!(event.git);
        assert!(event.directories.is_empty());
    }

    #[test]
    fn paths_under_dot_git_in_the_tree_and_outside_it_are_ignored() {
        let mut summary = ChangeSummary::default();
        summary.add(&root(), None, &root().join(".git/refs/heads/main"));
        summary.add(&root(), None, Path::new("/elsewhere/file.txt"));
        assert!(summary.is_empty());
    }

    #[test]
    fn too_many_folders_become_a_read_everything_request() {
        let mut summary = ChangeSummary::default();
        for index in 0..=MAX_LISTED_DIRECTORIES {
            summary.add(&root(), None, &root().join(format!("d{index}/file")));
        }

        let event = summary.into_event("/work/app");
        assert!(event.everything);
        assert!(event.directories.is_empty());
    }
}
