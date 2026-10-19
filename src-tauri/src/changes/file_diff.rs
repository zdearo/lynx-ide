//! The two sides of one file's diff: what HEAD holds and what the working tree holds now.

use std::path::Path;

use serde::Serialize;
use specta::Type;

use crate::error::AppError;
use crate::files::file_content::{MAX_TEXT_BYTES, read_text};
use crate::files::files_error::FilesError;
use crate::files::path_inside_worktree::check_shape;
use crate::git::GitError;
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

/// `original_path` is where a renamed file was in HEAD; without it the file has the same path there.
pub fn read_diff(
    root: &Path,
    relative: &str,
    original_path: Option<&str>,
) -> Result<FileDiff, AppError> {
    check_shape(root, relative)?;
    let head_path = original_path.unwrap_or(relative);
    check_shape(root, head_path)?;

    let original = match git(root)
        .arg("show")
        .arg(format!("HEAD:{head_path}"))
        .run_bytes()
    {
        Ok(bytes) => Some(text_of(bytes, head_path)?),
        Err(error) if is_missing_in_head(&error) => None,
        Err(error) => return Err(error.into()),
    };

    let modified = match read_text(root, relative) {
        Ok(file) => Some(file.content),
        Err(FilesError::NotFound(_)) => None,
        Err(error) => return Err(error.into()),
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

        let diff = read_diff(&repo.path, "README.md", None).expect("diff");

        assert_eq!(diff.original.as_deref(), Some("hello\n"));
        assert_eq!(diff.modified.as_deref(), Some("hello\nworld\n"));
    }

    #[test]
    fn the_trailing_newline_is_kept() {
        let repo = TestRepository::new();
        fs::write(repo.path.join("README.md"), "hello").expect("edit");

        let diff = read_diff(&repo.path, "README.md", None).expect("diff");

        assert_eq!(diff.original.as_deref(), Some("hello\n"));
        assert_eq!(diff.modified.as_deref(), Some("hello"));
    }

    #[test]
    fn a_new_file_has_no_original() {
        let repo = TestRepository::new();
        fs::write(repo.path.join("new.txt"), "fresh\n").expect("write");

        let diff = read_diff(&repo.path, "new.txt", None).expect("diff");

        assert_eq!(diff.original, None);
        assert_eq!(diff.modified.as_deref(), Some("fresh\n"));
    }

    #[test]
    fn a_deleted_file_has_no_modified_side() {
        let repo = TestRepository::new();
        fs::remove_file(repo.path.join("README.md")).expect("remove");

        let diff = read_diff(&repo.path, "README.md", None).expect("diff");

        assert_eq!(diff.original.as_deref(), Some("hello\n"));
        assert_eq!(diff.modified, None);
    }

    #[test]
    fn a_renamed_file_reads_its_original_from_the_old_path() {
        let repo = TestRepository::new();
        repo.git(&["mv", "README.md", "GUIDE.md"]);

        let diff = read_diff(&repo.path, "GUIDE.md", Some("README.md")).expect("diff");

        assert_eq!(diff.original.as_deref(), Some("hello\n"));
        assert_eq!(diff.modified.as_deref(), Some("hello\n"));
    }

    #[test]
    fn a_binary_file_is_refused() {
        let repo = TestRepository::new();
        fs::write(repo.path.join("blob.bin"), [0u8, 1, 2, 3]).expect("write");

        let error = read_diff(&repo.path, "blob.bin", None).expect_err("binary");

        assert!(matches!(error, AppError::Files(FilesError::Binary(_))));
    }

    #[test]
    fn a_path_outside_the_worktree_is_refused() {
        let repo = TestRepository::new();

        let error = read_diff(&repo.path, "../secret", None).expect_err("outside");

        assert!(matches!(error, AppError::Files(FilesError::InvalidPath(_))));
    }
}
