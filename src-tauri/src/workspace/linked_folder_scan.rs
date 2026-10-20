use std::fs;
use std::path::{Path, PathBuf};

/// Deeper than any real project layout; a deeper tree is cut and reported as truncated.
const MAX_DEPTH: usize = 8;
/// Directories read in one scan, so linking a huge folder (a home directory, say) stays bounded.
const MAX_DIRECTORIES: usize = 20_000;
const MAX_REPOS: usize = 2_000;

/// Plain directories that only hold generated files. A repo with one of these names is still found.
const NEVER_DESCEND: [&str; 4] = ["node_modules", "target", "venv", "__pycache__"];

/// A directory under the linked one that holds at least one repository below it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedDirectory {
    pub path: PathBuf,
    pub parent: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedRepo {
    pub path: PathBuf,
    pub parent: PathBuf,
}

/// What a scan found. `directories` lists a parent before its children.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ScanTree {
    pub directories: Vec<ScannedDirectory>,
    pub repos: Vec<ScannedRepo>,
    /// A limit cut the scan short, so absence from it proves nothing.
    pub truncated: bool,
}

/// True when `path` is itself a repository (its `.git` is a folder).
pub fn is_repository(path: &Path) -> bool {
    path.join(".git").is_dir()
}

/// Walks `root` and returns the repositories under it with the directories that lead to them.
/// A repository ends its branch: nothing inside it is read. Directories without a repository
/// below are left out, so a linked home folder does not fill the sidebar with empty folders.
/// Only the existence of `.git` is checked and no process runs, so a scan stays cheap.
pub fn scan(root: &Path) -> ScanTree {
    let mut walk = Walk::default();
    walk.visit(root, 0);
    let mut kept = Vec::new();
    for (directory, has_repos) in walk.directories {
        if has_repos {
            kept.push(directory);
        }
    }
    ScanTree {
        directories: kept,
        repos: walk.repos,
        truncated: walk.truncated,
    }
}

#[derive(Default)]
struct Walk {
    /// Every directory read, in visiting order, with whether a repository turned up below it.
    directories: Vec<(ScannedDirectory, bool)>,
    repos: Vec<ScannedRepo>,
    visited: usize,
    truncated: bool,
}

impl Walk {
    /// Returns whether a repository was found at or below `directory`.
    fn visit(&mut self, directory: &Path, depth: usize) -> bool {
        if depth >= MAX_DEPTH || self.visited >= MAX_DIRECTORIES {
            self.truncated = true;
            return false;
        }
        self.visited += 1;

        let Ok(entries) = fs::read_dir(directory) else {
            return false;
        };
        let mut children: Vec<(String, PathBuf)> = entries
            .flatten()
            // `file_type` does not follow symlinks: a link could loop or leave the linked folder.
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .map(|entry| {
                (
                    entry.file_name().to_string_lossy().into_owned(),
                    entry.path(),
                )
            })
            .collect();
        children.sort();

        let mut found = false;
        for (name, path) in children {
            // Hidden folders are tooling (`.cache`, `.nvm`), never the user's projects.
            if name.starts_with('.') {
                continue;
            }
            if self.repos.len() >= MAX_REPOS {
                self.truncated = true;
                break;
            }
            if is_repository(&path) {
                self.repos.push(ScannedRepo {
                    path,
                    parent: directory.to_owned(),
                });
                found = true;
                continue;
            }
            // A `.git` file marks a linked worktree or a submodule: not a project of its own.
            if path.join(".git").exists() {
                continue;
            }
            if NEVER_DESCEND.contains(&name.as_str()) {
                continue;
            }

            let index = self.directories.len();
            self.directories.push((
                ScannedDirectory {
                    path: path.clone(),
                    parent: directory.to_owned(),
                },
                false,
            ));
            if self.visit(&path, depth + 1) {
                self.directories[index].1 = true;
                found = true;
            }
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_repo(path: &Path) {
        fs::create_dir_all(path.join(".git")).expect("make repo");
    }

    fn names(paths: impl Iterator<Item = PathBuf>, root: &Path) -> Vec<String> {
        paths
            .map(|path| {
                path.strip_prefix(root)
                    .expect("inside root")
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect()
    }

    #[test]
    fn finds_repos_and_the_folders_that_lead_to_them() {
        let root = tempfile::tempdir().expect("root");
        make_repo(&root.path().join("lynx"));
        make_repo(&root.path().join("clients/acme"));
        make_repo(&root.path().join("clients/beta/api"));
        fs::create_dir_all(root.path().join("notes/ideas")).expect("empty tree");

        let tree = scan(root.path());

        assert_eq!(
            names(tree.repos.iter().map(|r| r.path.clone()), root.path()),
            ["clients/acme", "clients/beta/api", "lynx"]
        );
        assert_eq!(
            names(tree.directories.iter().map(|d| d.path.clone()), root.path()),
            ["clients", "clients/beta"]
        );
        assert!(!tree.truncated);
    }

    #[test]
    fn a_parent_is_listed_before_its_children() {
        let root = tempfile::tempdir().expect("root");
        make_repo(&root.path().join("a/b/c/repo"));

        let tree = scan(root.path());

        let order = names(tree.directories.iter().map(|d| d.path.clone()), root.path());
        assert_eq!(order, ["a", "a/b", "a/b/c"]);
        assert_eq!(tree.directories[1].parent, root.path().join("a"));
        assert_eq!(tree.repos[0].parent, root.path().join("a/b/c"));
    }

    #[test]
    fn stops_at_the_first_repository_on_each_branch() {
        let root = tempfile::tempdir().expect("root");
        make_repo(&root.path().join("outer"));
        make_repo(&root.path().join("outer/vendor/inner"));

        let tree = scan(root.path());

        assert_eq!(tree.repos.len(), 1);
        assert_eq!(tree.repos[0].path, root.path().join("outer"));
        assert!(tree.directories.is_empty());
    }

    #[test]
    fn ignores_linked_worktrees_and_submodules() {
        let root = tempfile::tempdir().expect("root");
        make_repo(&root.path().join("app"));
        let worktree = root.path().join("app-worktrees/feature-x");
        fs::create_dir_all(&worktree).expect("worktree dir");
        fs::write(worktree.join(".git"), "gitdir: ../../app/.git/worktrees/x").expect("git file");

        let tree = scan(root.path());

        assert_eq!(tree.repos.len(), 1);
        assert_eq!(tree.repos[0].path, root.path().join("app"));
        // The worktrees folder holds no project, so it is not mirrored.
        assert!(tree.directories.is_empty());
    }

    #[test]
    fn skips_hidden_and_generated_folders_but_not_a_repo_with_that_name() {
        let root = tempfile::tempdir().expect("root");
        make_repo(&root.path().join(".cache/repo"));
        make_repo(&root.path().join("node_modules/pkg"));
        make_repo(&root.path().join("target"));

        let tree = scan(root.path());

        assert_eq!(
            names(tree.repos.iter().map(|r| r.path.clone()), root.path()),
            ["target"]
        );
    }

    #[cfg(unix)]
    #[test]
    fn does_not_follow_symlinks() {
        let root = tempfile::tempdir().expect("root");
        let outside = tempfile::tempdir().expect("outside");
        make_repo(&outside.path().join("elsewhere"));
        std::os::unix::fs::symlink(outside.path(), root.path().join("link")).expect("symlink");
        std::os::unix::fs::symlink(root.path(), root.path().join("loop")).expect("loop");

        let tree = scan(root.path());

        assert!(tree.repos.is_empty());
        assert!(tree.directories.is_empty());
    }

    #[test]
    fn a_too_deep_tree_is_cut_and_reported() {
        let root = tempfile::tempdir().expect("root");
        let mut deep = root.path().to_owned();
        for level in 0..=MAX_DEPTH {
            deep = deep.join(format!("d{level}"));
        }
        make_repo(&deep.join("repo"));

        let tree = scan(root.path());

        assert!(tree.truncated);
        assert!(tree.repos.is_empty());
    }

    #[test]
    fn a_missing_root_yields_an_empty_tree() {
        let root = tempfile::tempdir().expect("root");

        assert_eq!(scan(&root.path().join("gone")), ScanTree::default());
    }
}
