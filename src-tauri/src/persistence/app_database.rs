use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, ErrorCode, OptionalExtension, params};

use crate::workspace::linked_folder_sync::LinkedPlan;
use crate::workspace::workspace_models::{Folder, Repo, WorktreeMeta};

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
        let mut statement = connection
            .prepare("SELECT id, name, path, folder_id, linked_root_id, missing FROM repos ORDER BY created_at, rowid")?;
        let repos = statement
            .query_map([], repo_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(repos)
    }

    pub fn find_repo(&self, repo_id: &str) -> Result<Option<Repo>, DatabaseError> {
        Ok(self
            .lock()
            .query_row(
                "SELECT id, name, path, folder_id, linked_root_id, missing FROM repos WHERE id = ?1",
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
            "SELECT id, name, path, folder_id, linked_root_id, missing FROM repos WHERE path = ?1",
            [&candidate.path],
            repo_from_row,
        )?;
        transaction.commit()?;
        Ok(repo)
    }

    pub fn list_folders(&self) -> Result<Vec<Folder>, DatabaseError> {
        let connection = self.lock();
        let mut statement = connection.prepare(
            "SELECT id, name, parent_id, collapsed, source_path, linked_root_id, missing FROM folders ORDER BY created_at, rowid",
        )?;
        let folders = statement
            .query_map([], folder_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(folders)
    }

    pub fn find_folder(&self, folder_id: &str) -> Result<Option<Folder>, DatabaseError> {
        Ok(self
            .lock()
            .query_row(
                "SELECT id, name, parent_id, collapsed, source_path, linked_root_id, missing FROM folders WHERE id = ?1",
                [folder_id],
                folder_from_row,
            )
            .optional()?)
    }

    pub fn insert_folder(&self, folder: &Folder) -> Result<(), DatabaseError> {
        self.lock().execute(
            "INSERT INTO folders
                 (id, name, parent_id, collapsed, source_path, linked_root_id, missing, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                folder.id,
                folder.name,
                folder.parent_id,
                i64::from(folder.collapsed),
                folder.source_path,
                folder.linked_root_id,
                i64::from(folder.missing),
                now_seconds()
            ],
        )?;
        Ok(())
    }

    /// Returns whether a folder was found and renamed.
    pub fn rename_folder(&self, folder_id: &str, name: &str) -> Result<bool, DatabaseError> {
        let changed = self.lock().execute(
            "UPDATE folders SET name = ?2 WHERE id = ?1",
            params![folder_id, name],
        )?;
        Ok(changed > 0)
    }

    pub fn move_folder(
        &self,
        folder_id: &str,
        parent_id: Option<&str>,
    ) -> Result<bool, DatabaseError> {
        let changed = self.lock().execute(
            "UPDATE folders SET parent_id = ?2 WHERE id = ?1",
            params![folder_id, parent_id],
        )?;
        Ok(changed > 0)
    }

    pub fn set_folder_collapsed(
        &self,
        folder_id: &str,
        collapsed: bool,
    ) -> Result<bool, DatabaseError> {
        let changed = self.lock().execute(
            "UPDATE folders SET collapsed = ?2 WHERE id = ?1",
            params![folder_id, i64::from(collapsed)],
        )?;
        Ok(changed > 0)
    }

    /// Deletes a folder and moves what was inside it one level up, so nothing
    /// is lost. Returns whether the folder existed. All in one transaction.
    pub fn delete_folder(&self, folder_id: &str) -> Result<bool, DatabaseError> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let parent: Option<Option<String>> = transaction
            .query_row(
                "SELECT parent_id FROM folders WHERE id = ?1",
                [folder_id],
                |row| row.get(0),
            )
            .optional()?;
        let Some(parent) = parent else {
            return Ok(false);
        };
        transaction.execute(
            "UPDATE folders SET parent_id = ?2 WHERE parent_id = ?1",
            params![folder_id, parent],
        )?;
        transaction.execute(
            "UPDATE repos SET folder_id = ?2 WHERE folder_id = ?1",
            params![folder_id, parent],
        )?;
        // Without its linked folder nothing would keep the rest in sync, so it becomes ordinary.
        clear_link(&transaction, folder_id)?;
        transaction.execute("DELETE FROM folders WHERE id = ?1", [folder_id])?;
        transaction.commit()?;
        Ok(true)
    }

    /// Stops mirroring the directory: the linked folder, the folders under it and its repos stay
    /// as ordinary ones. Returns whether the folder existed.
    pub fn unlink_folder(&self, folder_id: &str) -> Result<bool, DatabaseError> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let exists = transaction
            .query_row("SELECT 1 FROM folders WHERE id = ?1", [folder_id], |_| {
                Ok(())
            })
            .optional()?
            .is_some();
        if exists {
            clear_link(&transaction, folder_id)?;
        }
        transaction.commit()?;
        Ok(exists)
    }

    /// Paths the user removed from a linked folder, which its scans must skip.
    pub fn linked_exclusions(&self, root_id: &str) -> Result<HashSet<String>, DatabaseError> {
        let connection = self.lock();
        let mut statement =
            connection.prepare("SELECT path FROM linked_exclusions WHERE root_id = ?1")?;
        let paths = statement
            .query_map([root_id], |row| row.get::<_, String>(0))?
            .collect::<Result<HashSet<_>, _>>()?;
        Ok(paths)
    }

    /// Applies a scan's result in one transaction, so a crash never leaves half a tree.
    pub fn apply_linked_plan(&self, plan: &LinkedPlan) -> Result<(), DatabaseError> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let created_at = now_seconds();
        for folder in &plan.new_folders {
            transaction.execute(
                "INSERT INTO folders (id, name, parent_id, collapsed, source_path, linked_root_id, created_at)
                 VALUES (?1, ?2, ?3, 0, ?4, ?5, ?6)",
                params![
                    folder.id,
                    folder.name,
                    folder.parent_id,
                    folder.source_path,
                    plan.root_id,
                    created_at
                ],
            )?;
        }
        for repo in &plan.new_repos {
            // A path registered in the meantime keeps its row.
            transaction.execute(
                "INSERT INTO repos (id, name, path, folder_id, linked_root_id, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(path) DO NOTHING",
                params![
                    repo.id,
                    repo.name,
                    repo.path,
                    repo.folder_id,
                    plan.root_id,
                    created_at
                ],
            )?;
        }
        for (folder_id, missing) in &plan.folder_missing {
            transaction.execute(
                "UPDATE folders SET missing = ?2 WHERE id = ?1",
                params![folder_id, i64::from(*missing)],
            )?;
        }
        for (repo_id, missing) in &plan.repo_missing {
            transaction.execute(
                "UPDATE repos SET missing = ?2 WHERE id = ?1",
                params![repo_id, i64::from(*missing)],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Returns whether a repo was found and moved.
    pub fn move_repo(&self, repo_id: &str, folder_id: Option<&str>) -> Result<bool, DatabaseError> {
        let changed = self.lock().execute(
            "UPDATE repos SET folder_id = ?2 WHERE id = ?1",
            params![repo_id, folder_id],
        )?;
        Ok(changed > 0)
    }

    /// Returns whether a repo was removed. Its worktree metadata goes with it. A repo that a
    /// linked folder found is also excluded, or the next scan would add it back.
    pub fn delete_repo(&self, repo_id: &str) -> Result<bool, DatabaseError> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let found: Option<(String, Option<String>)> = transaction
            .query_row(
                "SELECT path, linked_root_id FROM repos WHERE id = ?1",
                [repo_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((path, linked_root_id)) = found else {
            return Ok(false);
        };
        if let Some(root_id) = linked_root_id {
            transaction.execute(
                "INSERT OR IGNORE INTO linked_exclusions (root_id, path) VALUES (?1, ?2)",
                params![root_id, path],
            )?;
        }
        transaction.execute("DELETE FROM repos WHERE id = ?1", [repo_id])?;
        transaction.commit()?;
        Ok(true)
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

/// Turns everything a linked folder found into ordinary folders and repos.
fn clear_link(transaction: &rusqlite::Transaction<'_>, root_id: &str) -> rusqlite::Result<()> {
    transaction.execute(
        "UPDATE folders SET source_path = NULL, linked_root_id = NULL, missing = 0
         WHERE linked_root_id = ?1",
        [root_id],
    )?;
    transaction.execute(
        "UPDATE repos SET linked_root_id = NULL, missing = 0 WHERE linked_root_id = ?1",
        [root_id],
    )?;
    transaction.execute(
        "DELETE FROM linked_exclusions WHERE root_id = ?1",
        [root_id],
    )?;
    Ok(())
}

fn repo_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Repo> {
    Ok(Repo {
        id: row.get(0)?,
        name: row.get(1)?,
        path: row.get(2)?,
        folder_id: row.get(3)?,
        linked_root_id: row.get(4)?,
        missing: row.get::<_, i64>(5)? != 0,
    })
}

fn folder_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Folder> {
    Ok(Folder {
        id: row.get(0)?,
        name: row.get(1)?,
        parent_id: row.get(2)?,
        collapsed: row.get::<_, i64>(3)? != 0,
        source_path: row.get(4)?,
        linked_root_id: row.get(5)?,
        missing: row.get::<_, i64>(6)? != 0,
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
            folder_id: None,
            linked_root_id: None,
            missing: false,
        }
    }

    fn folder(id: &str, parent: Option<&str>) -> Folder {
        Folder {
            id: id.to_owned(),
            name: id.to_owned(),
            parent_id: parent.map(str::to_owned),
            collapsed: false,
            source_path: None,
            linked_root_id: None,
            missing: false,
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

    #[test]
    fn folders_round_trip_with_their_parent_and_collapsed_state() {
        let (database, _dir) = database();
        database.insert_folder(&folder("work", None)).expect("work");
        database
            .insert_folder(&folder("client", Some("work")))
            .expect("client");
        database
            .set_folder_collapsed("work", true)
            .expect("collapse");

        let listed = database.list_folders().expect("list");

        assert_eq!(
            listed.iter().map(|f| f.id.as_str()).collect::<Vec<_>>(),
            ["work", "client"]
        );
        assert_eq!(listed[1].parent_id.as_deref(), Some("work"));
        assert!(listed[0].collapsed);
    }

    #[test]
    fn a_repo_can_be_moved_into_a_folder_and_back_to_the_top() {
        let (database, _dir) = database();
        database
            .get_or_insert_repo(&repo("a", "/work/a"))
            .expect("repo");
        database
            .insert_folder(&folder("work", None))
            .expect("folder");

        assert!(database.move_repo("a", Some("work")).expect("move in"));
        assert_eq!(
            database
                .find_repo("a")
                .expect("find")
                .expect("repo")
                .folder_id
                .as_deref(),
            Some("work")
        );

        assert!(database.move_repo("a", None).expect("move out"));
        assert_eq!(
            database
                .find_repo("a")
                .expect("find")
                .expect("repo")
                .folder_id,
            None
        );
    }

    #[test]
    fn a_repo_cannot_be_placed_in_a_folder_that_does_not_exist() {
        let (database, _dir) = database();
        database
            .get_or_insert_repo(&repo("a", "/work/a"))
            .expect("repo");

        assert!(
            database.move_repo("a", Some("ghost")).is_err(),
            "foreign keys must be enforced"
        );
    }

    #[test]
    fn deleting_a_folder_moves_its_folders_and_repos_up_one_level() {
        let (database, _dir) = database();
        database.insert_folder(&folder("work", None)).expect("work");
        database
            .insert_folder(&folder("client", Some("work")))
            .expect("client");
        database
            .insert_folder(&folder("deep", Some("client")))
            .expect("deep");
        database
            .get_or_insert_repo(&repo("a", "/work/a"))
            .expect("repo a");
        database.move_repo("a", Some("client")).expect("place a");

        assert!(database.delete_folder("client").expect("delete"));

        let folders = database.list_folders().expect("list");
        assert_eq!(
            folders.iter().map(|f| f.id.as_str()).collect::<Vec<_>>(),
            ["work", "deep"]
        );
        assert_eq!(folders[1].parent_id.as_deref(), Some("work"));
        assert_eq!(
            database
                .find_repo("a")
                .expect("find")
                .expect("repo")
                .folder_id
                .as_deref(),
            Some("work")
        );
    }

    #[test]
    fn deleting_a_top_level_folder_moves_its_contents_to_the_top() {
        let (database, _dir) = database();
        database.insert_folder(&folder("work", None)).expect("work");
        database
            .insert_folder(&folder("client", Some("work")))
            .expect("client");
        database
            .get_or_insert_repo(&repo("a", "/work/a"))
            .expect("repo");
        database.move_repo("a", Some("work")).expect("place");

        database.delete_folder("work").expect("delete");

        assert_eq!(database.list_folders().expect("list")[0].parent_id, None);
        assert_eq!(
            database
                .find_repo("a")
                .expect("find")
                .expect("repo")
                .folder_id,
            None
        );
    }

    #[test]
    fn deleting_a_missing_folder_reports_it() {
        let (database, _dir) = database();
        assert!(!database.delete_folder("nope").expect("delete"));
    }
}

/// A terminal session the app knows: enough to find it again, and to resume its agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalSessionRecord {
    pub id: String,
    pub worktree_path: String,
    /// The agent's harness name, or None for a plain shell.
    pub harness: Option<String>,
    pub command: Option<String>,
    /// The shell the session runs, so it can be started again the same way.
    pub shell: String,
    /// What the agent's own conversation is called, to resume it.
    pub resume_id: Option<String>,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub exit_code: Option<i32>,
}

fn terminal_session_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TerminalSessionRecord> {
    Ok(TerminalSessionRecord {
        id: row.get(0)?,
        worktree_path: row.get(1)?,
        harness: row.get(2)?,
        command: row.get(3)?,
        shell: row.get(4)?,
        resume_id: row.get(5)?,
        started_at: row.get(6)?,
        ended_at: row.get(7)?,
        exit_code: row.get(8)?,
    })
}

const TERMINAL_SESSION_COLUMNS: &str =
    "id, worktree_path, harness, command, shell, resume_id, started_at, ended_at, exit_code";

impl AppDatabase {
    pub fn insert_terminal_session(
        &self,
        record: &TerminalSessionRecord,
    ) -> Result<(), DatabaseError> {
        self.lock().execute(
            "INSERT INTO terminal_sessions (id, worktree_path, harness, command, shell, resume_id, started_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                record.id,
                record.worktree_path,
                record.harness,
                record.command,
                record.shell,
                record.resume_id,
                record.started_at,
            ],
        )?;
        Ok(())
    }

    pub fn set_terminal_resume_id(&self, id: &str, resume_id: &str) -> Result<(), DatabaseError> {
        self.lock().execute(
            "UPDATE terminal_sessions SET resume_id = ?2 WHERE id = ?1",
            params![id, resume_id],
        )?;
        Ok(())
    }

    /// The agent a session runs, once it is known: a shell that starts one by hand names it later.
    pub fn set_terminal_harness(&self, id: &str, harness: &str) -> Result<(), DatabaseError> {
        self.lock().execute(
            "UPDATE terminal_sessions SET harness = ?2 WHERE id = ?1",
            params![id, harness],
        )?;
        Ok(())
    }

    /// The first end wins: a session that already ended keeps its time and code.
    pub fn end_terminal_session(
        &self,
        id: &str,
        exit_code: Option<i32>,
    ) -> Result<(), DatabaseError> {
        self.lock().execute(
            "UPDATE terminal_sessions SET ended_at = ?2, exit_code = ?3
             WHERE id = ?1 AND ended_at IS NULL",
            params![id, now_seconds(), exit_code],
        )?;
        Ok(())
    }

    #[cfg(test)]
    pub fn terminal_session(
        &self,
        id: &str,
    ) -> Result<Option<TerminalSessionRecord>, DatabaseError> {
        let connection = self.lock();
        let mut statement = connection.prepare(&format!(
            "SELECT {TERMINAL_SESSION_COLUMNS} FROM terminal_sessions WHERE id = ?1"
        ))?;
        Ok(statement.query_row([id], terminal_session_row).optional()?)
    }

    /// Sessions the app started that have not ended yet, oldest first.
    pub fn open_terminal_sessions(&self) -> Result<Vec<TerminalSessionRecord>, DatabaseError> {
        let connection = self.lock();
        let mut statement = connection.prepare(&format!(
            "SELECT {TERMINAL_SESSION_COLUMNS} FROM terminal_sessions
             WHERE ended_at IS NULL ORDER BY started_at, id"
        ))?;
        let rows = statement.query_map([], terminal_session_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Ended agent sessions of one worktree that can be resumed, newest first.
    pub fn resumable_terminal_sessions(
        &self,
        worktree_path: &str,
    ) -> Result<Vec<TerminalSessionRecord>, DatabaseError> {
        let connection = self.lock();
        // Only the newest session of a conversation is offered: resuming it makes a newer one.
        let mut statement = connection.prepare(&format!(
            "SELECT {TERMINAL_SESSION_COLUMNS} FROM terminal_sessions AS t
             WHERE worktree_path = ?1 AND ended_at IS NOT NULL
               AND harness IS NOT NULL AND resume_id IS NOT NULL
               AND NOT EXISTS (
                 SELECT 1 FROM terminal_sessions AS newer
                 WHERE newer.harness = t.harness AND newer.resume_id = t.resume_id
                   AND newer.started_at > t.started_at
               )
             ORDER BY started_at DESC, id LIMIT 10"
        ))?;
        let rows = statement.query_map([worktree_path], terminal_session_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Takes an ended agent conversation off the resumable list: every ended record of it. The
    /// conversation itself stays in the agent. Returns whether `id` was such a record.
    pub fn forget_resumable_session(&self, id: &str) -> Result<bool, DatabaseError> {
        let deleted = self.lock().execute(
            "DELETE FROM terminal_sessions
             WHERE ended_at IS NOT NULL AND (harness, resume_id) IN (
               SELECT harness, resume_id FROM terminal_sessions
               WHERE id = ?1 AND ended_at IS NOT NULL AND resume_id IS NOT NULL
             )",
            params![id],
        )?;
        Ok(deleted > 0)
    }
}

#[cfg(test)]
mod terminal_session_tests {
    use super::*;

    fn database() -> AppDatabase {
        let dir = tempfile::tempdir().expect("temp dir");
        AppDatabase::open(&dir.path().join("app.db")).expect("open")
    }

    fn record(id: &str, harness: Option<&str>) -> TerminalSessionRecord {
        TerminalSessionRecord {
            id: id.to_owned(),
            worktree_path: "/work/a".to_owned(),
            harness: harness.map(str::to_owned),
            command: None,
            shell: "/bin/sh".to_owned(),
            resume_id: None,
            started_at: 10,
            ended_at: None,
            exit_code: None,
        }
    }

    #[test]
    fn a_session_is_open_until_it_ends_and_the_first_end_wins() {
        let database = database();
        database
            .insert_terminal_session(&record("s1", None))
            .expect("insert");
        assert_eq!(database.open_terminal_sessions().expect("open").len(), 1);

        database.end_terminal_session("s1", Some(3)).expect("end");
        database
            .end_terminal_session("s1", Some(9))
            .expect("end again");

        assert!(database.open_terminal_sessions().expect("open").is_empty());
        let ended = database.terminal_session("s1").expect("read").expect("row");
        assert_eq!(ended.exit_code, Some(3), "the first end keeps its code");
    }

    #[test]
    fn only_ended_agent_sessions_with_a_conversation_can_be_resumed() {
        let database = database();
        database
            .insert_terminal_session(&record("agent", Some("claude")))
            .expect("insert");
        database
            .set_terminal_resume_id("agent", "abc")
            .expect("resume id");
        database
            .insert_terminal_session(&record("shell", None))
            .expect("insert");
        database
            .set_terminal_resume_id("shell", "xyz")
            .expect("resume id");
        database
            .insert_terminal_session(&record("unnamed", Some("codex")))
            .expect("insert");
        database.end_terminal_session("agent", None).expect("end");
        database.end_terminal_session("shell", None).expect("end");
        database.end_terminal_session("unnamed", None).expect("end");

        let resumable = database
            .resumable_terminal_sessions("/work/a")
            .expect("list");
        assert_eq!(resumable.len(), 1);
        assert_eq!(resumable[0].id, "agent");
        assert!(
            database
                .resumable_terminal_sessions("/work/b")
                .expect("list")
                .is_empty()
        );
    }

    #[test]
    fn resuming_a_conversation_hides_the_session_it_came_from() {
        let database = database();
        database
            .insert_terminal_session(&record("first", Some("codex")))
            .expect("insert");
        database
            .set_terminal_resume_id("first", "conv")
            .expect("resume id");
        database.end_terminal_session("first", None).expect("end");
        assert_eq!(
            database
                .resumable_terminal_sessions("/work/a")
                .expect("list")
                .len(),
            1
        );

        // The resumed session has the same conversation, and is newer; it is still running.
        let mut resumed = record("second", Some("codex"));
        resumed.started_at = 20;
        database.insert_terminal_session(&resumed).expect("insert");
        database
            .set_terminal_resume_id("second", "conv")
            .expect("resume id");
        assert!(
            database
                .resumable_terminal_sessions("/work/a")
                .expect("list")
                .is_empty()
        );

        database.end_terminal_session("second", None).expect("end");
        let offered = database
            .resumable_terminal_sessions("/work/a")
            .expect("list");
        assert_eq!(offered.len(), 1);
        assert_eq!(offered[0].id, "second");
    }

    #[test]
    fn a_conversation_taken_off_the_list_stays_off_and_a_running_one_is_kept() {
        let database = database();
        database
            .insert_terminal_session(&record("old", Some("claude")))
            .expect("insert");
        database
            .set_terminal_resume_id("old", "conv")
            .expect("resume id");
        database.end_terminal_session("old", None).expect("end");
        let mut newer = record("new", Some("claude"));
        newer.started_at = 20;
        database.insert_terminal_session(&newer).expect("insert");
        database
            .set_terminal_resume_id("new", "conv")
            .expect("resume id");
        database.end_terminal_session("new", None).expect("end");
        database
            .insert_terminal_session(&record("live", Some("claude")))
            .expect("insert");
        database
            .set_terminal_resume_id("live", "other")
            .expect("resume id");

        assert!(database.forget_resumable_session("new").expect("forget"));

        assert!(
            database
                .resumable_terminal_sessions("/work/a")
                .expect("list")
                .is_empty()
        );
        assert!(database.terminal_session("old").expect("read").is_none());
        assert!(
            database.terminal_session("live").expect("read").is_some(),
            "a running session is not removed"
        );
        assert!(!database.forget_resumable_session("live").expect("forget"));
        assert!(
            !database
                .forget_resumable_session("missing")
                .expect("forget")
        );
    }
}
