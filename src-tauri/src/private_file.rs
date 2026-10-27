//! Files only their owner can read, written in one step so a reader never sees half of one.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Replaces the file in one step. On Unix only its owner can read it; Windows keeps the
/// permissions of the folder it is in.
pub fn write(file: &Path, contents: &[u8]) -> io::Result<()> {
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = temporary_name(file);
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut handle = options.open(&temporary)?;
    handle.write_all(contents)?;
    handle.sync_all()?;
    drop(handle);
    fs::rename(&temporary, file)
}

/// The temporary sits next to the file, so the rename never crosses a filesystem.
fn temporary_name(file: &Path) -> PathBuf {
    let mut name = file.as_os_str().to_owned();
    name.push(".tmp");
    PathBuf::from(name)
}
