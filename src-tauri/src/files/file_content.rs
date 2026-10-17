//! Reads and writes the text of one file in a worktree. A file Lynx cannot edit as text (too large,
//! binary, not UTF-8) is refused rather than shown garbled, and a write never replaces a file that
//! changed on disk since it was read, unless the caller says to.

use std::fs;
use std::io::Write;
use std::path::Path;

use serde::Serialize;
use sha2::{Digest, Sha256};
use specta::Type;

use super::files_error::FilesError;
use super::path_inside_worktree::resolve_inside;

/// The largest file opened as text.
pub const MAX_TEXT_BYTES: u64 = 2 * 1024 * 1024;
/// How much of a file is looked at for a NUL byte, which text does not have.
const BINARY_SNIFF_BYTES: usize = 8000;

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FileText {
    pub content: String,
    /// Identifies this exact content. A write names the hash it started from, to notice a change on disk.
    pub hash: String,
}

fn hash_of(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn io_error(relative: &str, source: std::io::Error) -> FilesError {
    FilesError::Io {
        path: relative.to_owned(),
        source,
    }
}

pub fn read_text(root: &Path, relative: &str) -> Result<FileText, FilesError> {
    let target = resolve_inside(root, relative)?;
    let metadata = fs::metadata(&target).map_err(|source| io_error(relative, source))?;
    if !metadata.is_file() {
        return Err(FilesError::NotAFile(relative.to_owned()));
    }
    if metadata.len() > MAX_TEXT_BYTES {
        return Err(FilesError::TooLarge {
            path: relative.to_owned(),
            limit: MAX_TEXT_BYTES,
        });
    }
    let bytes = fs::read(&target).map_err(|source| io_error(relative, source))?;
    let sniffed = &bytes[..bytes.len().min(BINARY_SNIFF_BYTES)];
    if sniffed.contains(&0) {
        return Err(FilesError::Binary(relative.to_owned()));
    }
    let hash = hash_of(&bytes);
    let content = String::from_utf8(bytes).map_err(|_| FilesError::Binary(relative.to_owned()))?;
    Ok(FileText { content, hash })
}

/// Replaces the file's text. With `expected_hash`, the file must still be what the caller read.
pub fn write_text(
    root: &Path,
    relative: &str,
    content: &str,
    expected_hash: Option<&str>,
) -> Result<FileText, FilesError> {
    let target = resolve_inside(root, relative)?;
    let metadata = fs::metadata(&target).map_err(|source| io_error(relative, source))?;
    if !metadata.is_file() {
        return Err(FilesError::NotAFile(relative.to_owned()));
    }
    if content.len() as u64 > MAX_TEXT_BYTES {
        return Err(FilesError::TooLarge {
            path: relative.to_owned(),
            limit: MAX_TEXT_BYTES,
        });
    }
    if let Some(expected) = expected_hash {
        let current = fs::read(&target).map_err(|source| io_error(relative, source))?;
        if hash_of(&current) != expected {
            return Err(FilesError::Changed(relative.to_owned()));
        }
    }

    // A temp file beside the target and a rename, so an interrupted write never leaves half a file.
    let name = target
        .file_name()
        .map_or_else(|| "file".into(), |name| name.to_string_lossy().into_owned());
    let temporary = target.with_file_name(format!(".{name}.lynx-tmp"));
    let result = (|| -> std::io::Result<()> {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        fs::set_permissions(&temporary, metadata.permissions())?;
        fs::rename(&temporary, &target)
    })();
    if let Err(source) = result {
        let _ = fs::remove_file(&temporary);
        return Err(io_error(relative, source));
    }
    Ok(FileText {
        content: content.to_owned(),
        hash: hash_of(content.as_bytes()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("temp dir");
        fs::create_dir_all(dir.path().join("src")).expect("mkdir");
        fs::write(dir.path().join("src/main.rs"), "fn main() {}\n").expect("write");
        dir
    }

    #[test]
    fn reads_a_text_file_with_its_hash() {
        let dir = tree();
        let file = read_text(dir.path(), "src/main.rs").expect("read");
        assert_eq!(file.content, "fn main() {}\n");
        assert_eq!(file.hash, hash_of(b"fn main() {}\n"));
    }

    #[test]
    fn refuses_paths_that_leave_the_worktree_and_folders() {
        let dir = tree();
        assert!(matches!(
            read_text(dir.path(), "../outside"),
            Err(FilesError::InvalidPath(_))
        ));
        assert!(matches!(
            read_text(dir.path(), "src"),
            Err(FilesError::NotAFile(_))
        ));
        assert!(matches!(
            read_text(dir.path(), "missing.txt"),
            Err(FilesError::NotFound(_))
        ));
    }

    #[test]
    fn refuses_binary_and_non_utf8_files() {
        let dir = tree();
        fs::write(dir.path().join("a.bin"), [1u8, 0, 2]).expect("write");
        fs::write(dir.path().join("b.txt"), [0xffu8, 0xfe, 0x41]).expect("write");
        assert!(matches!(
            read_text(dir.path(), "a.bin"),
            Err(FilesError::Binary(_))
        ));
        assert!(matches!(
            read_text(dir.path(), "b.txt"),
            Err(FilesError::Binary(_))
        ));
    }

    #[test]
    fn refuses_a_file_over_the_limit() {
        let dir = tree();
        let big = vec![b'a'; usize::try_from(MAX_TEXT_BYTES).expect("fits") + 1];
        fs::write(dir.path().join("big.txt"), big).expect("write");
        assert!(matches!(
            read_text(dir.path(), "big.txt"),
            Err(FilesError::TooLarge { .. })
        ));
    }

    #[test]
    fn a_write_replaces_the_text_and_leaves_no_temp_file() {
        let dir = tree();
        let read = read_text(dir.path(), "src/main.rs").expect("read");

        let written = write_text(
            dir.path(),
            "src/main.rs",
            "fn main() { 1 }\n",
            Some(&read.hash),
        )
        .expect("write");

        assert_eq!(
            fs::read_to_string(dir.path().join("src/main.rs")).expect("read"),
            "fn main() { 1 }\n"
        );
        assert_eq!(written.hash, hash_of(b"fn main() { 1 }\n"));
        assert!(!dir.path().join("src/.main.rs.lynx-tmp").exists());
    }

    #[test]
    fn a_write_over_a_file_that_changed_on_disk_is_refused_unless_forced() {
        let dir = tree();
        let read = read_text(dir.path(), "src/main.rs").expect("read");
        fs::write(dir.path().join("src/main.rs"), "changed elsewhere\n").expect("write");

        let refused = write_text(dir.path(), "src/main.rs", "mine\n", Some(&read.hash));
        assert!(matches!(refused, Err(FilesError::Changed(_))));
        assert_eq!(
            fs::read_to_string(dir.path().join("src/main.rs")).expect("read"),
            "changed elsewhere\n"
        );

        write_text(dir.path(), "src/main.rs", "mine\n", None).expect("forced");
        assert_eq!(
            fs::read_to_string(dir.path().join("src/main.rs")).expect("read"),
            "mine\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_write_keeps_the_file_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tree();
        let path = dir.path().join("src/main.rs");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod");

        write_text(dir.path(), "src/main.rs", "x\n", None).expect("write");

        let mode = fs::metadata(&path).expect("metadata").permissions().mode();
        assert_eq!(mode & 0o777, 0o755);
    }
}
