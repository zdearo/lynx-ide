use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::error::AppError;
use crate::git::git_command::git;

use super::workspace_service::MAX_NAME_LENGTH;

/// Characters that Windows, macOS and Linux refuse in a folder name.
const FORBIDDEN_CHARS: [char; 9] = ['/', '\\', ':', '*', '?', '"', '<', '>', '|'];

/// Creates `parent/name` as an empty git repository and returns its path. Only
/// `git init` runs, so the repo has no commit and no branch yet.
pub fn create_empty_project(parent: &Path, name: &str) -> Result<PathBuf, AppError> {
    let name = validate_project_name(name)?;
    if !parent.exists() {
        return Err(AppError::LocationNotFound(parent.display().to_string()));
    }
    if !parent.is_dir() {
        return Err(AppError::LocationNotADirectory(
            parent.display().to_string(),
        ));
    }

    let target = parent.join(&name);
    // create_dir, not create_dir_all: an existing folder is never reused silently.
    fs::create_dir(&target).map_err(|source| match source.kind() {
        io::ErrorKind::AlreadyExists => AppError::AlreadyExists(target.display().to_string()),
        _ => AppError::CreateDirectory {
            path: target.display().to_string(),
            source,
        },
    })?;

    if let Err(error) = git(&target).args(["init", "--quiet"]).run() {
        // A leftover empty folder would take the name for the next attempt.
        let _ = fs::remove_dir(&target);
        return Err(error.into());
    }
    Ok(target)
}

/// The name becomes the folder and the repo name, so it must be one plain folder name.
fn validate_project_name(raw: &str) -> Result<String, AppError> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(AppError::InvalidName(
            "the project name is empty".to_owned(),
        ));
    }
    // Windows does not allow a trailing dot, and "." and ".." are not names at all.
    if name.ends_with('.') {
        return Err(AppError::InvalidName(format!(
            "{name} is not a valid folder name"
        )));
    }
    if name.chars().count() > MAX_NAME_LENGTH {
        return Err(AppError::InvalidName(format!(
            "the project name is longer than {MAX_NAME_LENGTH} characters"
        )));
    }
    if name
        .chars()
        .any(|c| c.is_control() || FORBIDDEN_CHARS.contains(&c))
    {
        return Err(AppError::InvalidName(
            "the project name cannot contain / \\ : * ? \" < > | or control characters".to_owned(),
        ));
    }
    Ok(name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_an_empty_repository_inside_the_parent() {
        let parent = tempfile::tempdir().expect("parent");

        let created = create_empty_project(parent.path(), "my-app").expect("create");

        assert_eq!(created, parent.path().join("my-app"));
        assert!(created.join(".git").is_dir());
    }

    #[test]
    fn rejects_names_that_are_not_one_plain_folder_name() {
        let parent = tempfile::tempdir().expect("parent");
        for name in [
            "", "   ", ".", "..", "a/b", "a\\b", "bad:name", "tail.", "x*y",
        ] {
            assert!(
                matches!(
                    create_empty_project(parent.path(), name),
                    Err(AppError::InvalidName(_))
                ),
                "{name:?} should be refused"
            );
        }
        assert_eq!(fs::read_dir(parent.path()).expect("read").count(), 0);
    }

    #[test]
    fn never_reuses_an_existing_folder() {
        let parent = tempfile::tempdir().expect("parent");
        fs::create_dir(parent.path().join("taken")).expect("make folder");

        let result = create_empty_project(parent.path(), "taken");

        assert!(matches!(result, Err(AppError::AlreadyExists(_))));
        assert!(!parent.path().join("taken").join(".git").exists());
    }

    #[test]
    fn refuses_a_location_that_is_missing_or_a_file() {
        let root = tempfile::tempdir().expect("root");
        let file = root.path().join("notes.txt");
        fs::write(&file, "not a folder").expect("write file");

        let missing = create_empty_project(&root.path().join("missing"), "app");
        let not_a_folder = create_empty_project(&file, "app");

        assert!(matches!(missing, Err(AppError::LocationNotFound(_))));
        assert!(matches!(
            not_a_folder,
            Err(AppError::LocationNotADirectory(_))
        ));
    }
}
