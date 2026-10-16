use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, ErrorCode, OptionalExtension, params};

use crate::workspace::workspace_models::{Repo, WorktreeMeta};

use super::schema_migrations::{self, MigrationError};

#[derive(Debug, thiserror::Error)]
pub enum DatabaseError {
    #[error("could not prepare the app data directory {path}: {source}")]
    DataDirectory {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("app database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Migration(#[from] MigrationError),
}

/// App state in a single SQLite file. One connection behind a mutex is enough
/// while all writers live in this process; revisit when the PTY daemon (a second
/// process) starts writing, which WAL mode already tolerates.
pub struct AppDatabase {
    connection: Mutex<Connection>,
}

impl AppDatabase {
    pub fn open(path: &Path) -> Result<Self, DatabaseError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| DatabaseError::DataDirectory {
                path: parent.to_owned(),
                source,
            })?;
        }

        let connection = match open_and_migrate(path) {
            Err(DatabaseError::Sqlite(error)) if is_not_a_database(&error) => {
                // Keep the unreadable file for recovery instead of overwriting it.
                set_aside(path)?;
                open_and_migrate(path)?
            }
            other => other?,
        };
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn list_repos(&self) -> Result<Vec<Repo>, DatabaseError> {
        let connection = self.lock();
        let mut statement =
            connection.prepare("SELECT id, name, path FROM repos ORDER BY created_at, rowid")?;
        let repos = statement
            .query_map([], repo_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(repos)
    }

    pub fn find_repo(&self, repo_id: &str) -> Result<Option<Repo>, DatabaseError> {
        Ok(self
            .lock()
            .query_row(
                "SELECT id, name, path FROM repos WHERE id = ?1",
                [repo_id],
                repo_from_row,
            )
            .optional()?)
    }

    /// Registers `candidate`, or returns the repo already registered for that
    /// path. Done in SQL so two concurrent adds cannot create duplicates.
    pub fn get_or_insert_repo(&self, candidate: &Repo) -> Result<Repo, DatabaseError> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO repos (id, name, path, created_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(path) DO NOTHING",
            params![candidate.id, candidate.name, candidate.path, now_seconds()],
        )?;
        let repo = transaction.query_row(
            "SELECT id, name, path FROM repos WHERE path = ?1",
            [&candidate.path],
            repo_from_row,
        )?;
        transaction.commit()?;
        Ok(repo)
    }

    /// Returns whether a repo was removed. Its worktree metadata goes with it.
    pub fn delete_repo(&self, repo_id: &str) -> Result<bool, DatabaseError> {
        let removed = self
            .lock()
            .execute("DELETE FROM repos WHERE id = ?1", [repo_id])?;
        Ok(removed > 0)
    }

    pub fn worktree_meta(
        &self,
        repo_id: &str,
    ) -> Result<BTreeMap<String, WorktreeMeta>, DatabaseError> {
        let connection = self.lock();
        let mut statement =
            connection.prepare("SELECT path, base_ref FROM worktree_meta WHERE repo_id = ?1")?;
        let rows = statement.query_map([repo_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                WorktreeMeta {
                    base_ref: row.get(1)?,
                },
            ))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn set_worktree_meta(
        &self,
        repo_id: &str,
        path: &str,
        meta: &WorktreeMeta,
    ) -> Result<(), DatabaseError> {
        self.lock().execute(
            "INSERT INTO worktree_meta (repo_id, path, base_ref, created_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(repo_id, path) DO UPDATE SET base_ref = excluded.base_ref",
            params![repo_id, path, meta.base_ref, now_seconds()],
        )?;
        Ok(())
    }

    pub fn delete_worktree_meta(&self, repo_id: &str, path: &str) -> Result<(), DatabaseError> {
        self.lock().execute(
            "DELETE FROM worktree_meta WHERE repo_id = ?1 AND path = ?2",
            params![repo_id, path],
        )?;
        Ok(())
    }

    fn lock(&self) -> MutexGuard<'_, Connection> {
        // Each statement is atomic in SQLite, so a poisoned lock holds no half-applied state.
        self.connection
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

fn open_and_migrate(path: &Path) -> Result<Connection, DatabaseError> {
    let mut connection = Connection::open(path)?;
    connection.busy_timeout(Duration::from_secs(5))?;
    // WAL lets readers and a writer overlap and is safe with NORMAL syncing.
    connection
        .pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get::<_, String>(0))?;
    connection.pragma_update(None, "synchronous", "NORMAL")?;
    connection.pragma_update(None, "foreign_keys", true)?;
    schema_migrations::migrate(&mut connection)?;
    Ok(connection)
}

fn is_not_a_database(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(failure, _) if failure.code == ErrorCode::NotADatabase
    )
}

fn set_aside(path: &Path) -> Result<(), DatabaseError> {
    let seconds = now_seconds();
    for suffix in ["", "-wal", "-shm"] {
        let mut source = path.as_os_str().to_owned();
        source.push(suffix);
        let source = PathBuf::from(source);
        if !source.exists() {
            continue;
        }
        let mut target = path.as_os_str().to_owned();
        target.push(format!(".corrupt-{seconds}{suffix}"));
        fs::rename(&source, PathBuf::from(target)).map_err(|error| {
            DatabaseError::DataDirectory {
                path: source,
                source: error,
            }
        })?;
    }
    Ok(())
}

fn repo_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Repo> {
    Ok(Repo {
        id: row.get(0)?,
        name: row.get(1)?,
        path: row.get(2)?,
    })
}

fn now_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn database() -> (AppDatabase, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("temp dir");
        let database = AppDatabase::open(&dir.path().join("nested").join("app.db")).expect("open");
        (database, dir)
    }

    fn repo(id: &str, path: &str) -> Repo {
        Repo {
            id: id.to_owned(),
            name: id.to_owned(),
            path: path.to_owned(),
        }
    }

    fn meta(base_ref: &str) -> WorktreeMeta {
        WorktreeMeta {
            base_ref: base_ref.to_owned(),
        }
    }

    #[test]
    fn starts_empty_and_creates_missing_directories() {
        let (database, _dir) = database();
        assert!(database.list_repos().expect("list").is_empty());
    }

    #[test]
    fn data_survives_reopening() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("app.db");
        let first = AppDatabase::open(&path).expect("open");
        first
            .get_or_insert_repo(&repo("a", "/work/a"))
            .expect("insert");
        first
            .set_worktree_meta("a", "/work/a-worktrees/x", &meta("main"))
            .expect("meta");
        drop(first);

        let reopened = AppDatabase::open(&path).expect("reopen");

        assert_eq!(
            reopened.list_repos().expect("list"),
            vec![repo("a", "/work/a")]
        );
        assert_eq!(
            reopened.worktree_meta("a").expect("meta")["/work/a-worktrees/x"],
            meta("main")
        );
    }

    #[test]
    fn registering_the_same_path_twice_returns_the_first_repo() {
        let (database, _dir) = database();
        let first = database
            .get_or_insert_repo(&repo("a", "/work/a"))
            .expect("first");

        let second = database
            .get_or_insert_repo(&repo("b", "/work/a"))
            .expect("second");

        assert_eq!(second, first);
        assert_eq!(database.list_repos().expect("list").len(), 1);
    }

    #[test]
    fn repos_are_listed_in_registration_order() {
        let (database, _dir) = database();
        for id in ["c", "a", "b"] {
            database
                .get_or_insert_repo(&repo(id, &format!("/work/{id}")))
                .expect("insert");
        }

        let ids: Vec<_> = database
            .list_repos()
            .expect("list")
            .into_iter()
            .map(|r| r.id)
            .collect();

        assert_eq!(ids, ["c", "a", "b"]);
    }

    #[test]
    fn deleting_a_repo_removes_its_worktree_metadata_only() {
        let (database, _dir) = database();
        database
            .get_or_insert_repo(&repo("a", "/work/a"))
            .expect("a");
        database
            .get_or_insert_repo(&repo("b", "/work/b"))
            .expect("b");
        database
            .set_worktree_meta("a", "/wt/a1", &meta("main"))
            .expect("a meta");
        database
            .set_worktree_meta("b", "/wt/b1", &meta("dev"))
            .expect("b meta");

        assert!(database.delete_repo("a").expect("delete"));
        assert!(!database.delete_repo("a").expect("delete again"));

        assert!(database.find_repo("a").expect("find").is_none());
        assert!(database.worktree_meta("a").expect("meta a").is_empty());
        assert_eq!(database.worktree_meta("b").expect("meta b").len(), 1);
    }

    #[test]
    fn worktree_meta_requires_an_existing_repo() {
        let (database, _dir) = database();

        let result = database.set_worktree_meta("ghost", "/wt/x", &meta("main"));

        assert!(result.is_err(), "foreign keys must be enforced");
    }

    #[test]
    fn setting_meta_again_updates_it_and_deleting_removes_it() {
        let (database, _dir) = database();
        database
            .get_or_insert_repo(&repo("a", "/work/a"))
            .expect("a");
        database
            .set_worktree_meta("a", "/wt/x", &meta("main"))
            .expect("first");
        database
            .set_worktree_meta("a", "/wt/x", &meta("dev"))
            .expect("second");

        assert_eq!(
            database.worktree_meta("a").expect("meta")["/wt/x"],
            meta("dev")
        );

        database.delete_worktree_meta("a", "/wt/x").expect("delete");
        assert!(database.worktree_meta("a").expect("meta").is_empty());
    }

    #[test]
    fn unreadable_file_is_set_aside_not_overwritten() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("app.db");
        fs::write(
            &path,
            "this is definitely not a sqlite database, just text padding",
        )
        .expect("write");

        let database = AppDatabase::open(&path).expect("open");

        assert!(database.list_repos().expect("list").is_empty());
        let backups: Vec<_> = fs::read_dir(dir.path())
            .expect("list dir")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains("corrupt"))
            .collect();
        assert_eq!(backups.len(), 1);
        assert!(
            fs::read_to_string(backups[0].path())
                .expect("read backup")
                .starts_with("this is definitely not")
        );
    }

    #[test]
    fn a_database_from_a_newer_build_is_refused_and_left_untouched() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("app.db");
        drop(AppDatabase::open(&path).expect("create"));
        Connection::open(&path)
            .expect("raw open")
            .pragma_update(None, "user_version", 999)
            .expect("bump version");

        let result = AppDatabase::open(&path);

        assert!(matches!(
            result,
            Err(DatabaseError::Migration(MigrationError::SchemaTooNew {
                found: 999,
                ..
            }))
        ));
        assert!(path.exists());
    }
}
