use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use super::files_error::FilesError;

/// Turns a worktree-relative path from the frontend into a real path, refusing
/// anything that could leave the worktree: absolute paths, `..`, and symlinks
/// that lead elsewhere. The empty string means the worktree root itself.
pub fn resolve_inside(root: &Path, relative: &str) -> Result<PathBuf, FilesError> {
    let relative_path = Path::new(relative);
    let all_plain = relative_path
        .components()
        .all(|component| matches!(component, Component::Normal(_)));
    // `Path` treats backslash as a separator only on Windows; reject it everywhere
    // so the same string means the same thing on every platform.
    if !all_plain || relative.contains('\\') || relative.contains('\0') {
        return Err(FilesError::InvalidPath(relative.to_owned()));
    }

    let canonical_root = fs::canonicalize(root).map_err(|source| FilesError::Io {
        path: root.display().to_string(),
        source,
    })?;
    let target = canonical_root.join(relative_path);
    let canonical_target = fs::canonicalize(&target).map_err(|source| match source.kind() {
        io::ErrorKind::NotFound => FilesError::NotFound(relative.to_owned()),
        _ => FilesError::Io {
            path: relative.to_owned(),
            source,
        },
    })?;

    if canonical_target.starts_with(&canonical_root) {
        Ok(canonical_target)
    } else {
        Err(FilesError::OutsideWorktree(relative.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("temp dir");
        fs::create_dir_all(dir.path().join("src/nested")).expect("mkdir");
        fs::write(dir.path().join("src/main.rs"), "fn main() {}").expect("write");
        dir
    }

    #[test]
    fn the_empty_path_is_the_root() {
        let dir = tree();
        let root = fs::canonicalize(dir.path()).expect("canonical");
        assert_eq!(resolve_inside(dir.path(), "").expect("root"), root);
    }

    #[test]
    fn resolves_files_and_folders_inside() {
        let dir = tree();
        let root = fs::canonicalize(dir.path()).expect("canonical");
        assert_eq!(
            resolve_inside(dir.path(), "src/nested").expect("dir"),
            root.join("src/nested")
        );
        assert_eq!(
            resolve_inside(dir.path(), "src/main.rs").expect("file"),
            root.join("src/main.rs")
        );
    }

    #[test]
    fn refuses_parent_segments_absolute_paths_and_odd_separators() {
        let dir = tree();
        for bad in ["..", "../x", "src/../../x", "/etc", "src\\nested", "a\0b"] {
            assert!(
                matches!(
                    resolve_inside(dir.path(), bad),
                    Err(FilesError::InvalidPath(_))
                ),
                "{bad:?} should be refused"
            );
        }
    }

    #[test]
    fn reports_a_missing_path() {
        let dir = tree();
        assert!(matches!(
            resolve_inside(dir.path(), "nope"),
            Err(FilesError::NotFound(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn refuses_a_symlink_that_leads_outside() {
        let dir = tree();
        let outside = tempfile::tempdir().expect("outside");
        fs::write(outside.path().join("secret.txt"), "x").expect("write");
        std::os::unix::fs::symlink(outside.path(), dir.path().join("escape")).expect("symlink");

        assert!(matches!(
            resolve_inside(dir.path(), "escape"),
            Err(FilesError::OutsideWorktree(_))
        ));
        assert!(matches!(
            resolve_inside(dir.path(), "escape/secret.txt"),
            Err(FilesError::OutsideWorktree(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn allows_a_symlink_that_stays_inside() {
        let dir = tree();
        std::os::unix::fs::symlink(dir.path().join("src"), dir.path().join("alias"))
            .expect("symlink");

        let root = fs::canonicalize(dir.path()).expect("canonical");
        assert_eq!(
            resolve_inside(dir.path(), "alias").expect("inside"),
            root.join("src")
        );
    }
}
