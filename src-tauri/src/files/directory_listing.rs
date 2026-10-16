use std::fs;
use std::path::Path;

use serde::Serialize;
use specta::Type;

use crate::git::ignored_paths::ignored_paths;

use super::files_error::FilesError;
use super::path_inside_worktree::resolve_inside;

/// A very large folder (think `node_modules`) must not freeze the panel.
const MAX_ENTRIES: usize = 5_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum FileKind {
    Directory,
    File,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub name: String,
    pub kind: FileKind,
    pub symlink: bool,
    /// Git ignores it; shown, but quieter.
    pub ignored: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryListing {
    pub entries: Vec<FileEntry>,
    /// There were more than the panel shows.
    pub truncated: bool,
}

/// One folder of a worktree: directories first, then files, each group by name.
pub fn list_directory(root: &Path, relative: &str) -> Result<DirectoryListing, FilesError> {
    let directory = resolve_inside(root, relative)?;
    if !directory.is_dir() {
        return Err(FilesError::NotADirectory(relative.to_owned()));
    }

    let io_error = |source| FilesError::Io {
        path: relative.to_owned(),
        source,
    };
    let mut entries = Vec::new();
    for item in fs::read_dir(&directory).map_err(io_error)? {
        let item = item.map_err(io_error)?;
        let name = item.file_name().to_string_lossy().into_owned();
        // Git's own bookkeeping is never what anyone is looking for here.
        if name == ".git" {
            continue;
        }
        let Ok(file_type) = item.file_type() else {
            continue;
        };
        let symlink = file_type.is_symlink();
        // A link counts as a folder when what it points to is one; a broken link is a file.
        let is_directory = if symlink {
            fs::metadata(item.path()).is_ok_and(|metadata| metadata.is_dir())
        } else {
            file_type.is_dir()
        };
        entries.push(FileEntry {
            name,
            kind: if is_directory {
                FileKind::Directory
            } else {
                FileKind::File
            },
            symlink,
            ignored: false,
        });
    }

    entries.sort_by(|a, b| {
        let rank = |entry: &FileEntry| u8::from(entry.kind == FileKind::File);
        rank(a)
            .cmp(&rank(b))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
    });

    let truncated = entries.len() > MAX_ENTRIES;
    entries.truncate(MAX_ENTRIES);

    mark_ignored(root, relative, &mut entries);
    Ok(DirectoryListing { entries, truncated })
}

fn mark_ignored(root: &Path, relative: &str, entries: &mut [FileEntry]) {
    let prefix = if relative.is_empty() {
        String::new()
    } else {
        format!("{relative}/")
    };
    let asked: Vec<String> = entries
        .iter()
        .map(|entry| {
            let slash = if entry.kind == FileKind::Directory {
                "/"
            } else {
                ""
            };
            format!("{prefix}{}{slash}", entry.name)
        })
        .collect();
    let ignored = ignored_paths(root, &asked);
    for (entry, path) in entries.iter_mut().zip(&asked) {
        entry.ignored = ignored.contains(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::test_repository::TestRepository;

    fn names(listing: &DirectoryListing) -> Vec<&str> {
        listing
            .entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect()
    }

    fn repo_with_files() -> TestRepository {
        let repo = TestRepository::new();
        repo.commit_file(".gitignore", "node_modules/\n*.log\n", "ignore");
        fs::create_dir_all(repo.path.join("src/ui")).expect("mkdir");
        fs::create_dir_all(repo.path.join("node_modules/dep")).expect("mkdir");
        fs::create_dir(repo.path.join("Zdocs")).expect("mkdir");
        fs::write(repo.path.join("src/main.rs"), "").expect("write");
        fs::write(repo.path.join("Cargo.toml"), "").expect("write");
        fs::write(repo.path.join("debug.log"), "").expect("write");
        repo
    }

    #[test]
    fn lists_folders_first_then_files_each_by_name_ignoring_case() {
        let repo = repo_with_files();

        let listing = list_directory(&repo.path, "").expect("list");

        assert_eq!(
            names(&listing),
            [
                "node_modules",
                "src",
                "Zdocs",
                ".gitignore",
                "Cargo.toml",
                "debug.log",
                "README.md"
            ]
        );
        assert!(!listing.truncated);
    }

    #[test]
    fn never_shows_the_git_directory_or_file() {
        let repo = repo_with_files();
        assert!(!names(&list_directory(&repo.path, "").expect("list")).contains(&".git"));

        let linked = tempfile::tempdir().expect("temp dir");
        fs::write(linked.path().join(".git"), "gitdir: /elsewhere").expect("write");
        fs::write(linked.path().join("a.txt"), "").expect("write");
        assert_eq!(
            names(&list_directory(linked.path(), "").expect("list")),
            ["a.txt"]
        );
    }

    #[test]
    fn marks_what_git_ignores_and_nothing_else() {
        let repo = repo_with_files();

        let listing = list_directory(&repo.path, "").expect("list");
        let ignored: Vec<_> = listing
            .entries
            .iter()
            .filter(|e| e.ignored)
            .map(|e| e.name.as_str())
            .collect();

        assert_eq!(ignored, ["node_modules", "debug.log"]);
    }

    #[test]
    fn lists_a_nested_folder_and_marks_ignored_files_in_it() {
        let repo = repo_with_files();
        fs::write(repo.path.join("src/trace.log"), "").expect("write");

        let listing = list_directory(&repo.path, "src").expect("list");

        assert_eq!(names(&listing), ["ui", "main.rs", "trace.log"]);
        assert!(
            listing
                .entries
                .iter()
                .find(|e| e.name == "trace.log")
                .expect("entry")
                .ignored
        );
        assert!(
            !listing
                .entries
                .iter()
                .find(|e| e.name == "main.rs")
                .expect("entry")
                .ignored
        );
    }

    #[test]
    fn works_without_git_and_just_marks_nothing() {
        let dir = tempfile::tempdir().expect("temp dir");
        fs::write(dir.path().join("a.log"), "").expect("write");

        let listing = list_directory(dir.path(), "").expect("list");

        assert!(listing.entries.iter().all(|entry| !entry.ignored));
    }

    #[test]
    fn rejects_files_missing_paths_and_escapes() {
        let repo = repo_with_files();
        assert!(matches!(
            list_directory(&repo.path, "Cargo.toml"),
            Err(FilesError::NotADirectory(_))
        ));
        assert!(matches!(
            list_directory(&repo.path, "missing"),
            Err(FilesError::NotFound(_))
        ));
        assert!(matches!(
            list_directory(&repo.path, "../"),
            Err(FilesError::InvalidPath(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_flagged_and_classified_by_their_target() {
        let repo = repo_with_files();
        std::os::unix::fs::symlink(repo.path.join("src"), repo.path.join("link-to-dir"))
            .expect("symlink");
        std::os::unix::fs::symlink(repo.path.join("Cargo.toml"), repo.path.join("link-to-file"))
            .expect("symlink");
        std::os::unix::fs::symlink(repo.path.join("gone"), repo.path.join("broken"))
            .expect("symlink");

        let listing = list_directory(&repo.path, "").expect("list");
        let find = |name: &str| {
            listing
                .entries
                .iter()
                .find(|e| e.name == name)
                .expect("entry")
                .clone()
        };

        assert_eq!(
            (find("link-to-dir").kind, find("link-to-dir").symlink),
            (FileKind::Directory, true)
        );
        assert_eq!(
            (find("link-to-file").kind, find("link-to-file").symlink),
            (FileKind::File, true)
        );
        assert_eq!(
            (find("broken").kind, find("broken").symlink),
            (FileKind::File, true)
        );
    }

    #[test]
    fn a_huge_folder_is_cut_and_flagged() {
        let dir = tempfile::tempdir().expect("temp dir");
        for n in 0..(MAX_ENTRIES + 50) {
            fs::write(dir.path().join(format!("f{n:05}")), "").expect("write");
        }

        let listing = list_directory(dir.path(), "").expect("list");

        assert_eq!(listing.entries.len(), MAX_ENTRIES);
        assert!(listing.truncated);
    }
}
