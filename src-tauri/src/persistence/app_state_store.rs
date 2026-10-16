use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::workspace::workspace_models::{Repo, WorktreeMeta};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("could not access app state at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("could not serialize app state: {0}")]
    Serialize(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct StoredState {
    pub repos: Vec<Repo>,
    /// Worktree metadata keyed by repo id, then by worktree path.
    pub worktrees: BTreeMap<String, BTreeMap<String, WorktreeMeta>>,
}

/// App state kept as one JSON file. Writes go to a temp file first so a crash
/// mid-write never leaves a half-written state behind.
pub struct AppStateStore {
    path: PathBuf,
    state: Mutex<StoredState>,
}

impl AppStateStore {
    pub fn open(path: PathBuf) -> Result<Self, StoreError> {
        let state = load(&path)?;
        Ok(Self {
            path,
            state: Mutex::new(state),
        })
    }

    pub fn read<R>(&self, read: impl FnOnce(&StoredState) -> R) -> R {
        read(&self.lock())
    }

    /// Applies `change` and persists. If persisting fails the in-memory state is
    /// left untouched, so memory and disk never disagree.
    pub fn update<R>(&self, change: impl FnOnce(&mut StoredState) -> R) -> Result<R, StoreError> {
        let mut guard = self.lock();
        let mut next = guard.clone();
        let result = change(&mut next);
        write_atomically(&self.path, &next)?;
        *guard = next;
        Ok(result)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, StoredState> {
        // The state is only replaced as a whole, so a poisoned lock is still consistent.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn load(path: &Path) -> Result<StoredState, StoreError> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(StoredState::default()),
        Err(source) => {
            return Err(StoreError::Io {
                path: path.to_owned(),
                source,
            });
        }
    };

    match serde_json::from_str(&contents) {
        Ok(state) => Ok(state),
        Err(_) => {
            // Keep the unreadable file for recovery instead of overwriting it on the next save.
            let seconds = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_secs());
            let backup = path.with_extension(format!("json.corrupt-{seconds}"));
            fs::rename(path, &backup).map_err(|source| StoreError::Io {
                path: path.to_owned(),
                source,
            })?;
            Ok(StoredState::default())
        }
    }
}

fn write_atomically(path: &Path, state: &StoredState) -> Result<(), StoreError> {
    let io_error = |source| StoreError::Io {
        path: path.to_owned(),
        source,
    };

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, serde_json::to_vec_pretty(state)?).map_err(io_error)?;
    fs::rename(&temporary, path).map_err(io_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(id: &str) -> Repo {
        Repo {
            id: id.to_owned(),
            name: id.to_owned(),
            path: format!("/work/{id}"),
        }
    }

    #[test]
    fn missing_file_starts_empty() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = AppStateStore::open(dir.path().join("state.json")).expect("open");
        assert!(store.read(|state| state.repos.is_empty()));
    }

    #[test]
    fn updates_survive_reopening() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("nested").join("state.json");

        let store = AppStateStore::open(path.clone()).expect("open");
        store
            .update(|state| state.repos.push(repo("a")))
            .expect("update");

        let reopened = AppStateStore::open(path).expect("reopen");
        assert_eq!(reopened.read(|state| state.repos.clone()), vec![repo("a")]);
    }

    #[test]
    fn failed_write_leaves_state_unchanged() {
        let dir = tempfile::tempdir().expect("temp dir");
        // A directory in place of the file makes the final rename fail.
        let path = dir.path().join("state.json");
        fs::create_dir(&path).expect("block path");
        fs::write(path.join("keep"), "x").expect("make non-empty");

        let store = AppStateStore {
            path,
            state: Mutex::new(StoredState::default()),
        };
        let result = store.update(|state| state.repos.push(repo("a")));

        assert!(result.is_err());
        assert!(store.read(|state| state.repos.is_empty()));
    }

    #[test]
    fn unreadable_file_is_set_aside_not_overwritten() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("state.json");
        fs::write(&path, "{ not json").expect("write garbage");

        let store = AppStateStore::open(path.clone()).expect("open");

        assert!(store.read(|state| state.repos.is_empty()));
        let backups: Vec<_> = fs::read_dir(dir.path())
            .expect("list dir")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains("corrupt"))
            .collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(
            fs::read_to_string(backups[0].path()).expect("read backup"),
            "{ not json"
        );
    }
}
