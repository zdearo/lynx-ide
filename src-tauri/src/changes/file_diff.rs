//! The two sides of one file's diff: what HEAD holds and what the working tree holds now.

use std::path::Path;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::error::AppError;
use crate::files::file_content::{MAX_TEXT_BYTES, read_text};
use crate::files::files_error::FilesError;
use crate::files::path_inside_worktree::check_shape;
use crate::git::GitError;
use crate::pull_request::pull_request_create::base_info;

use super::branch_sync;
use crate::git::git_command::git;

/// A side that is `None` does not exist there: a new file has no original, a deleted one no modified.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    pub original: Option<String>,
    pub modified: Option<String>,
}

/// Git says a path is not in HEAD in a few ways, all of which mean "there is no original".
fn is_missing_in_head(error: &GitError) -> bool {
    [
        "does not exist in",
        "exists on disk, but not in",
        "invalid object name",
        "unknown revision",
        "bad revision",
        "not a valid object name",
    ]
    .iter()
    .any(|needle| error.stderr_contains(needle))
}

fn text_of(bytes: Vec<u8>, relative: &str) -> Result<String, FilesError> {
    if bytes.len() as u64 > MAX_TEXT_BYTES {
        return Err(FilesError::TooLarge {
            path: relative.to_owned(),
            limit: MAX_TEXT_BYTES,
        });
    }
    if bytes.contains(&0) {
        return Err(FilesError::Binary(relative.to_owned()));
    }
    String::from_utf8(bytes).map_err(|_| FilesError::Binary(relative.to_owned()))
}

/// What a file's diff compares. Every source gives the two texts the interface shows side by side.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DiffSource {
    /// HEAD against the working tree: everything the file changed since the last commit.
    Head,
    /// HEAD against the index: what the next commit would hold.
    Staged,
    /// The index against the working tree: what is not staged yet.
    Unstaged,
    /// Where the branch left the remote's default branch, against the working tree.
    Base,
    /// A commit against its first parent.
    Commit { hash: String },
}

/// A commit hash as git prints it: 7 to 40 hex digits. Nothing else is handed to `git show`.
pub fn is_commit_hash(text: &str) -> bool {
    (7..=40).contains(&text.len()) && text.chars().all(|c| c.is_ascii_hexdigit())
}

/// The text of `path` at a revision (`HEAD`, a hash, or empty for the index), `None` when the file
/// does not exist there.
fn show_text(root: &Path, revision: &str, path: &str) -> Result<Option<String>, AppError> {
    match git(root)
        .arg("show")
        .arg(format!("{revision}:{path}"))
        .run_bytes()
    {
        Ok(bytes) => Ok(Some(text_of(bytes, path)?)),
        Err(error) if is_missing_in_head(&error) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn working_text(root: &Path, relative: &str) -> Result<Option<String>, AppError> {
    match read_text(root, relative) {
        Ok(file) => Ok(Some(file.content)),
        Err(FilesError::NotFound(_)) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// The revision the branch forked from the remote's default branch, for the `Base` source.
fn merge_base(root: &Path) -> Result<String, AppError> {
    let sync = branch_sync::read(root)?;
    let (Some(branch), Some(remote)) = (sync.branch, sync.remote) else {
        return Err(AppError::ChangeRefused(
            "there is no remote branch to compare with",
        ));
    };
    let info = base_info(root, &remote, &branch)?;
    let base = format!("{}/{}", info.remote, info.default_base);
    Ok(git(root).args(["merge-base", "HEAD", &base]).run()?)
}

/// `original_path` is where a renamed file was before the change, for the `original` side.
pub fn read_diff(
    root: &Path,
    relative: &str,
    original_path: Option<&str>,
    source: &DiffSource,
) -> Result<FileDiff, AppError> {
    check_shape(root, relative)?;
    let before_path = original_path.unwrap_or(relative);
    check_shape(root, before_path)?;

    let (original, modified) = match source {
        DiffSource::Head => (
            show_text(root, "HEAD", before_path)?,
            working_text(root, relative)?,
        ),
        DiffSource::Staged => (
            show_text(root, "HEAD", before_path)?,
            show_text(root, "", relative)?,
        ),
        DiffSource::Unstaged => (
            show_text(root, "", relative)?,
            working_text(root, relative)?,
        ),
        DiffSource::Base => (
            show_text(root, &merge_base(root)?, before_path)?,
            working_text(root, relative)?,
        ),
        DiffSource::Commit { hash } => {
            if !is_commit_hash(hash) {
                return Err(AppError::ChangeRefused("that is not a commit hash"));
            }
            (
                show_text(root, &format!("{hash}^"), before_path)?,
                show_text(root, hash, relative)?,
            )
        }
    };
    Ok(FileDiff { original, modified })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::git::test_repository::TestRepository;

    #[test]
    fn a_modified_file_has_both_sides() {
        let repo = TestRepository::new();
        fs::write(repo.path.join("README.md"), "hello\nworld\n").expect("edit");

        let diff = read_diff(&repo.path, "README.md", None, &DiffSource::Head).expect("diff");

        assert_eq!(diff.original.as_deref(), Some("hello\n"));
        assert_eq!(diff.modified.as_deref(), Some("hello\nworld\n"));
    }

    #[test]
    fn the_trailing_newline_is_kept() {
        let repo = TestRepository::new();
        fs::write(repo.path.join("README.md"), "hello").expect("edit");

        let diff = read_diff(&repo.path, "README.md", None, &DiffSource::Head).expect("diff");

        assert_eq!(diff.original.as_deref(), Some("hello\n"));
        assert_eq!(diff.modified.as_deref(), Some("hello"));
    }

    #[test]
    fn a_new_file_has_no_original() {
        let repo = TestRepository::new();
        fs::write(repo.path.join("new.txt"), "fresh\n").expect("write");

        let diff = read_diff(&repo.path, "new.txt", None, &DiffSource::Head).expect("diff");

        assert_eq!(diff.original, None);
        assert_eq!(diff.modified.as_deref(), Some("fresh\n"));
    }

    #[test]
    fn a_deleted_file_has_no_modified_side() {
        let repo = TestRepository::new();
        fs::remove_file(repo.path.join("README.md")).expect("remove");

        let diff = read_diff(&repo.path, "README.md", None, &DiffSource::Head).expect("diff");

        assert_eq!(diff.original.as_deref(), Some("hello\n"));
        assert_eq!(diff.modified, None);
    }

    #[test]
    fn a_renamed_file_reads_its_original_from_the_old_path() {
        let repo = TestRepository::new();
        repo.git(&["mv", "README.md", "GUIDE.md"]);

        let diff =
            read_diff(&repo.path, "GUIDE.md", Some("README.md"), &DiffSource::Head).expect("diff");

        assert_eq!(diff.original.as_deref(), Some("hello\n"));
        assert_eq!(diff.modified.as_deref(), Some("hello\n"));
    }

    #[test]
    fn a_binary_file_is_refused() {
        let repo = TestRepository::new();
        fs::write(repo.path.join("blob.bin"), [0u8, 1, 2, 3]).expect("write");

        let error = read_diff(&repo.path, "blob.bin", None, &DiffSource::Head).expect_err("binary");

        assert!(matches!(error, AppError::Files(FilesError::Binary(_))));
    }

    #[test]
    fn a_path_outside_the_worktree_is_refused() {
        let repo = TestRepository::new();

        let error =
            read_diff(&repo.path, "../secret", None, &DiffSource::Head).expect_err("outside");

        assert!(matches!(error, AppError::Files(FilesError::InvalidPath(_))));
    }

    #[test]
    fn staged_and_unstaged_split_what_head_to_working_tree_joins() {
        let repo = TestRepository::new();
        fs::write(repo.path.join("README.md"), "staged\n").expect("write");
        repo.git(&["add", "README.md"]);
        fs::write(repo.path.join("README.md"), "staged and more\n").expect("write");

        let staged = read_diff(&repo.path, "README.md", None, &DiffSource::Staged).expect("staged");
        let unstaged =
            read_diff(&repo.path, "README.md", None, &DiffSource::Unstaged).expect("unstaged");

        assert_eq!(staged.original.as_deref(), Some("hello\n"));
        assert_eq!(staged.modified.as_deref(), Some("staged\n"));
        assert_eq!(unstaged.original.as_deref(), Some("staged\n"));
        assert_eq!(unstaged.modified.as_deref(), Some("staged and more\n"));
    }

    #[test]
    fn a_commit_diff_compares_it_with_its_first_parent_and_a_root_has_no_original() {
        let repo = TestRepository::new();
        repo.commit_file("README.md", "second\n", "second");
        let second = repo.git(&["rev-parse", "HEAD"]);
        let first = repo.git(&["rev-parse", "HEAD~1"]);

        let diff = read_diff(
            &repo.path,
            "README.md",
            None,
            &DiffSource::Commit { hash: second },
        )
        .expect("diff");
        assert_eq!(diff.original.as_deref(), Some("hello\n"));
        assert_eq!(diff.modified.as_deref(), Some("second\n"));

        let root = read_diff(
            &repo.path,
            "README.md",
            None,
            &DiffSource::Commit { hash: first },
        )
        .expect("root");
        assert_eq!(root.original, None);
        assert_eq!(root.modified.as_deref(), Some("hello\n"));
    }

    #[test]
    fn only_commit_hashes_reach_git_show() {
        let repo = TestRepository::new();

        let error = read_diff(
            &repo.path,
            "README.md",
            None,
            &DiffSource::Commit {
                hash: "--output=x".to_owned(),
            },
        )
        .expect_err("not a hash");

        assert!(matches!(error, AppError::ChangeRefused(_)));
        assert!(is_commit_hash("abc1234"));
        assert!(!is_commit_hash("abc123"));
        assert!(!is_commit_hash("zzzzzzzz"));
    }

    #[test]
    fn the_base_source_starts_from_where_the_branch_left_the_default_branch() {
        let repo = TestRepository::new().with_origin();
        repo.git(&["checkout", "-q", "-b", "feat/x"]);
        repo.commit_file("README.md", "on the branch\n", "change");

        let diff = read_diff(&repo.path, "README.md", None, &DiffSource::Base).expect("diff");

        assert_eq!(diff.original.as_deref(), Some("hello\n"));
        assert_eq!(diff.modified.as_deref(), Some("on the branch\n"));
    }
}
