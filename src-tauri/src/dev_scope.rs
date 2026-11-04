//! Where a debug build keeps its data: one folder per git branch, so it never shares the installed
//! app's database, config, bots or logs. Only reads the repository's HEAD; nothing is written.

use std::fs;
use std::path::{Path, PathBuf};

const DETACHED: &str = "detached";
const HEAD_PREFIX: &str = "ref: refs/heads/";
const MAX_SLUG_LENGTH: usize = 60;

/// The branch a HEAD file points at, as a folder name. None for a detached HEAD.
pub fn branch_of_git_dir(head_contents: &str) -> Option<String> {
    let branch = head_contents.trim().strip_prefix(HEAD_PREFIX)?;
    let slug = slugify(branch);
    (!slug.is_empty()).then_some(slug)
}

/// The branch checked out in `repo_root`, whether it is the main checkout or a linked worktree.
pub fn dev_branch(repo_root: &Path) -> String {
    current_branch(repo_root).unwrap_or_else(|| DETACHED.to_owned())
}

/// `home/.lynx-dev/<branch>`: the LYNX_HOME of a debug build on that branch.
pub fn dev_home(branch: &str, home: &Path) -> PathBuf {
    home.join(".lynx-dev").join(branch)
}

/// `app_data/dev/<branch>/app.db`: the database of a debug build on that branch.
pub fn dev_database(app_data: &Path, branch: &str) -> PathBuf {
    app_data.join("dev").join(branch).join("app.db")
}

fn current_branch(repo_root: &Path) -> Option<String> {
    let head = fs::read_to_string(git_dir_of(repo_root)?.join("HEAD")).ok()?;
    branch_of_git_dir(&head)
}

/// A linked worktree has a `.git` file with `gitdir: <path>` that points at its own git folder.
fn git_dir_of(repo_root: &Path) -> Option<PathBuf> {
    let dot_git = repo_root.join(".git");
    if dot_git.is_dir() {
        return Some(dot_git);
    }
    let pointer = fs::read_to_string(&dot_git).ok()?;
    let path = pointer.trim().strip_prefix("gitdir:")?.trim();
    Some(repo_root.join(path))
}

/// Lowercase ASCII letters and digits; every other character becomes '-'.
fn slugify(name: &str) -> String {
    let mapped: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let cut: String = mapped
        .trim_matches('-')
        .chars()
        .take(MAX_SLUG_LENGTH)
        .collect();
    cut.trim_end_matches('-').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_keeps_lowercase_letters_and_digits_and_turns_the_rest_into_dashes() {
        assert_eq!(
            slugify("feature/orchestration-upgrade"),
            "feature-orchestration-upgrade"
        );
        assert_eq!(slugify("Feat_Auth/V2!"), "feat-auth-v2");
        assert_eq!(slugify("--edge--"), "edge");
        assert_eq!(slugify("café"), "caf");
    }

    #[test]
    fn slugify_cuts_at_sixty_characters_without_a_trailing_dash() {
        let long = format!("{}-{}", "a".repeat(59), "b".repeat(10));
        assert_eq!(slugify(&long), "a".repeat(59));
        assert_eq!(slugify(&"x".repeat(100)).len(), MAX_SLUG_LENGTH);
    }

    #[test]
    fn a_branch_ref_becomes_its_slug() {
        assert_eq!(
            branch_of_git_dir("ref: refs/heads/feat/auth\n"),
            Some("feat-auth".to_owned())
        );
    }

    #[test]
    fn a_detached_head_or_another_ref_has_no_branch() {
        assert_eq!(
            branch_of_git_dir("3f2a9c1d8e7b6a5f4e3d2c1b0a9f8e7d6c5b4a39\n"),
            None
        );
        assert_eq!(branch_of_git_dir("ref: refs/remotes/origin/main\n"), None);
        assert_eq!(branch_of_git_dir("ref: refs/heads/---\n"), None);
    }

    #[test]
    fn a_folder_git_dir_gives_its_branch() {
        let repo = tempfile::tempdir().unwrap();
        let git = repo.path().join(".git");
        fs::create_dir_all(&git).unwrap();
        fs::write(git.join("HEAD"), "ref: refs/heads/feat/auth\n").unwrap();
        assert_eq!(dev_branch(repo.path()), "feat-auth");
    }

    #[test]
    fn a_worktree_file_gives_the_branch_of_its_own_git_folder() {
        let root = tempfile::tempdir().unwrap();
        let gitdir = root
            .path()
            .join("main")
            .join(".git")
            .join("worktrees")
            .join("wt");
        fs::create_dir_all(&gitdir).unwrap();
        fs::write(gitdir.join("HEAD"), "ref: refs/heads/feat/auth\n").unwrap();

        let relative = root.path().join("relative");
        fs::create_dir_all(&relative).unwrap();
        fs::write(relative.join(".git"), "gitdir: ../main/.git/worktrees/wt\n").unwrap();
        assert_eq!(dev_branch(&relative), "feat-auth");

        let absolute = root.path().join("absolute");
        fs::create_dir_all(&absolute).unwrap();
        fs::write(
            absolute.join(".git"),
            format!("gitdir: {}\n", gitdir.display()),
        )
        .unwrap();
        assert_eq!(dev_branch(&absolute), "feat-auth");
    }

    #[test]
    fn a_detached_head_a_missing_git_dir_or_no_repository_is_detached() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(dev_branch(root.path()), "detached");

        let detached = root.path().join("detached");
        fs::create_dir_all(detached.join(".git")).unwrap();
        fs::write(
            detached.join(".git").join("HEAD"),
            "3f2a9c1d8e7b6a5f4e3d2c1b0a9f8e7d6c5b4a39\n",
        )
        .unwrap();
        assert_eq!(dev_branch(&detached), "detached");

        let dangling = root.path().join("dangling");
        fs::create_dir_all(&dangling).unwrap();
        fs::write(dangling.join(".git"), "gitdir: ../nowhere\n").unwrap();
        assert_eq!(dev_branch(&dangling), "detached");
    }

    #[test]
    fn the_scope_paths_sit_under_the_home_and_the_app_data() {
        assert_eq!(
            dev_home("feat-auth", Path::new("/home/u")),
            Path::new("/home/u").join(".lynx-dev").join("feat-auth")
        );
        assert_eq!(
            dev_database(Path::new("/data/app"), "feat-auth"),
            Path::new("/data/app")
                .join("dev")
                .join("feat-auth")
                .join("app.db")
        );
    }
}
