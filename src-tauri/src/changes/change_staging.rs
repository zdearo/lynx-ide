//! Moves changed files into and out of the index, the way `git add` and `git reset` do.

use std::path::Path;

use crate::error::AppError;
use crate::files::path_inside_worktree::check_shape;
use crate::git::git_command::git;

/// Stages one changed file. A deleted file stages its deletion.
pub fn stage(root: &Path, relative: &str) -> Result<(), AppError> {
    check_shape(root, relative)?;
    git(root)
        .literal_paths()
        .args(["add", "-A", "--", relative])
        .run()?;
    Ok(())
}

/// Paths per `git add` call: a long section must stay under the command-line length limit.
const STAGE_BATCH: usize = 100;

/// Stages several changed files at once, as a section's "stage all" button does. An empty list
/// stages nothing: `git add` with no paths would stage the whole worktree.
pub fn stage_many(root: &Path, relatives: &[String]) -> Result<(), AppError> {
    for relative in relatives {
        check_shape(root, relative)?;
    }
    for batch in relatives.chunks(STAGE_BATCH) {
        git(root)
            .literal_paths()
            .args(["add", "-A", "--"])
            .args(batch)
            .run()?;
    }
    Ok(())
}

/// Takes a file out of the index and leaves the working tree alone. `original` is the old
/// path of a renamed file, which goes back to its HEAD version too. Works before the first commit.
pub fn unstage(root: &Path, relative: &str, original: Option<&str>) -> Result<(), AppError> {
    check_shape(root, relative)?;
    let mut paths = vec![relative];
    if let Some(original) = original {
        check_shape(root, original)?;
        paths.push(original);
    }
    git(root)
        .literal_paths()
        .args(["reset", "-q", "--"])
        .args(paths)
        .run()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::changes::change_list::{ChangeKind, list_changes};
    use crate::git::test_repository::TestRepository;

    #[test]
    fn stage_and_unstage_move_a_change_between_the_sections() {
        let repo = TestRepository::new();
        fs::write(repo.path.join("README.md"), "changed\n").expect("write");

        stage(&repo.path, "README.md").expect("stage");
        let files = list_changes(&repo.path).expect("list");
        assert_eq!(files[0].staged, Some(ChangeKind::Modified));
        assert_eq!(files[0].unstaged, None);

        unstage(&repo.path, "README.md", None).expect("unstage");
        let files = list_changes(&repo.path).expect("list");
        assert_eq!(files[0].staged, None);
        assert_eq!(files[0].unstaged, Some(ChangeKind::Modified));
    }

    #[test]
    fn staging_a_deletion_stages_the_removal() {
        let repo = TestRepository::new();
        fs::remove_file(repo.path.join("README.md")).expect("remove");

        stage(&repo.path, "README.md").expect("stage");

        let files = list_changes(&repo.path).expect("list");
        assert_eq!(files[0].staged, Some(ChangeKind::Deleted));
    }

    #[test]
    fn unstaging_works_before_the_first_commit() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path();
        crate::git::git_command::git(root)
            .arg("init")
            .arg("-q")
            .run()
            .expect("init");
        fs::write(root.join("new.txt"), "hi\n").expect("write");
        stage(root, "new.txt").expect("stage");

        unstage(root, "new.txt", None).expect("unstage");

        let files = list_changes(root).expect("list");
        assert_eq!(files[0].staged, None);
        assert_eq!(files[0].unstaged, Some(ChangeKind::Untracked));
    }

    #[test]
    fn stage_many_stages_each_listed_file_and_nothing_else() {
        let repo = TestRepository::new();
        fs::write(repo.path.join("README.md"), "changed\n").expect("write");
        fs::write(repo.path.join("new.txt"), "hi\n").expect("write");
        fs::write(repo.path.join("other.txt"), "left alone\n").expect("write");

        stage_many(&repo.path, &["README.md".to_owned(), "new.txt".to_owned()]).expect("stage");

        let files = list_changes(&repo.path).expect("list");
        let other = files
            .iter()
            .find(|file| file.path == "other.txt")
            .expect("other");
        assert_eq!(other.unstaged, Some(ChangeKind::Untracked));
        let readme = files
            .iter()
            .find(|file| file.path == "README.md")
            .expect("readme");
        assert_eq!(readme.staged, Some(ChangeKind::Modified));
    }

    #[test]
    fn stage_many_with_no_paths_stages_nothing() {
        let repo = TestRepository::new();
        fs::write(repo.path.join("new.txt"), "hi\n").expect("write");

        stage_many(&repo.path, &[]).expect("stage nothing");

        let files = list_changes(&repo.path).expect("list");
        assert_eq!(files[0].unstaged, Some(ChangeKind::Untracked));
    }

    #[test]
    fn paths_that_leave_the_worktree_are_refused() {
        let repo = TestRepository::new();

        assert!(stage(&repo.path, "../outside.txt").is_err());
        assert!(stage_many(&repo.path, &["/etc/passwd".to_owned()]).is_err());
        assert!(unstage(&repo.path, "README.md", Some("/etc/passwd")).is_err());
    }
}
