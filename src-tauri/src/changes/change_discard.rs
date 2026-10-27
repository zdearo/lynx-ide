//! Throws away working-tree changes. Destructive: the interface asks first, and this only acts on
//! what `git status` says is an unstaged change.

use std::path::Path;

use crate::error::AppError;
use crate::files::path_inside_worktree::check_shape;
use crate::git::git_command::git;

use super::change_list::{ChangeKind, list_changes};

/// Paths per git call, so a long list stays under the command-line length limit.
const BATCH: usize = 100;

/// A tracked file goes back to what the index holds (its staged version is kept); an untracked
/// file is deleted. A path with no unstaged change is left alone, and a conflict is refused.
pub fn discard(root: &Path, paths: &[String]) -> Result<(), AppError> {
    for path in paths {
        check_shape(root, path)?;
    }
    let changes = list_changes(root)?;
    let mut restore: Vec<&str> = Vec::new();
    let mut clean: Vec<&str> = Vec::new();
    for path in paths {
        let Some(file) = changes.iter().find(|file| &file.path == path) else {
            continue;
        };
        match file.unstaged {
            Some(ChangeKind::Conflicted) => {
                return Err(AppError::ChangeRefused(
                    "a file with a merge conflict cannot be discarded; resolve it first",
                ));
            }
            Some(ChangeKind::Untracked) => clean.push(path),
            Some(_) => restore.push(path),
            None => {}
        }
    }
    for batch in restore.chunks(BATCH) {
        git(root)
            .literal_paths()
            .args(["restore", "--worktree", "--"])
            .args(batch)
            .run()?;
    }
    for batch in clean.chunks(BATCH) {
        git(root)
            .literal_paths()
            .args(["clean", "-f", "-q", "--"])
            .args(batch)
            .run()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::git::test_repository::TestRepository;

    #[cfg(unix)]
    #[test]
    fn a_file_named_like_a_pattern_discards_only_itself() {
        let repo = TestRepository::new();
        fs::write(repo.path.join("*"), "star\n").expect("write");
        fs::write(repo.path.join("keep.txt"), "keep\n").expect("write");

        discard(&repo.path, &["*".to_owned()]).expect("discard");

        assert!(!repo.path.join("*").exists());
        assert!(repo.path.join("keep.txt").exists());
    }

    #[test]
    fn a_modified_file_goes_back_and_a_new_file_is_deleted() {
        let repo = TestRepository::new();
        fs::write(repo.path.join("README.md"), "changed\n").expect("write");
        fs::write(repo.path.join("new.txt"), "new\n").expect("write");

        discard(&repo.path, &["README.md".to_owned(), "new.txt".to_owned()]).expect("discard");

        assert_eq!(
            fs::read_to_string(repo.path.join("README.md")).expect("read"),
            "hello\n"
        );
        assert!(!repo.path.join("new.txt").exists());
    }

    #[test]
    fn a_deleted_file_comes_back() {
        let repo = TestRepository::new();
        fs::remove_file(repo.path.join("README.md")).expect("remove");

        discard(&repo.path, &["README.md".to_owned()]).expect("discard");

        assert!(repo.path.join("README.md").exists());
    }

    #[test]
    fn the_staged_version_survives_discarding_what_came_after() {
        let repo = TestRepository::new();
        fs::write(repo.path.join("README.md"), "staged\n").expect("write");
        repo.git(&["add", "README.md"]);
        fs::write(repo.path.join("README.md"), "staged and more\n").expect("write");

        discard(&repo.path, &["README.md".to_owned()]).expect("discard");

        assert_eq!(
            fs::read_to_string(repo.path.join("README.md")).expect("read"),
            "staged\n"
        );
        assert_eq!(repo.git(&["diff", "--cached", "--name-only"]), "README.md");
    }

    #[test]
    fn a_path_with_nothing_to_discard_or_outside_the_worktree_is_handled() {
        let repo = TestRepository::new();

        assert!(discard(&repo.path, &["README.md".to_owned()]).is_ok());
        assert!(discard(&repo.path, &["../outside".to_owned()]).is_err());
        assert_eq!(
            fs::read_to_string(repo.path.join("README.md")).expect("read"),
            "hello\n"
        );
    }

    #[test]
    fn a_conflicted_file_is_refused() {
        let repo = TestRepository::new();
        repo.git(&["checkout", "-q", "-b", "other"]);
        repo.commit_file("README.md", "from other\n", "other");
        repo.git(&["checkout", "-q", "main"]);
        repo.commit_file("README.md", "from main\n", "main");
        let _ = git(&repo.path).args(["merge", "other"]).run();

        let error = discard(&repo.path, &["README.md".to_owned()]).expect_err("conflict");

        assert!(matches!(error, AppError::ChangeRefused(_)));
    }
}
