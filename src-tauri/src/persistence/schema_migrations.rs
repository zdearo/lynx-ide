//! Forward-only schema migrations, tracked with SQLite's `user_version`.
//! Never edit a released migration: append a new one.

use rusqlite::Connection;

const MIGRATIONS: &[&str] = &[
    // 1: repos and what the app remembers about their worktrees
    "CREATE TABLE repos (
         id         TEXT PRIMARY KEY,
         name       TEXT NOT NULL,
         path       TEXT NOT NULL UNIQUE,
         created_at INTEGER NOT NULL
     );
     CREATE TABLE worktree_meta (
         repo_id    TEXT NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
         path       TEXT NOT NULL,
         base_ref   TEXT NOT NULL,
         created_at INTEGER NOT NULL,
         PRIMARY KEY (repo_id, path)
     );",
    // 2: folders that group repos and other folders. A repo without a folder is at the top level.
    "CREATE TABLE folders (
         id         TEXT PRIMARY KEY,
         name       TEXT NOT NULL,
         parent_id  TEXT REFERENCES folders(id),
         collapsed  INTEGER NOT NULL DEFAULT 0,
         created_at INTEGER NOT NULL
     );
     ALTER TABLE repos ADD COLUMN folder_id TEXT REFERENCES folders(id);",
    // 3: folders linked to a directory on disk. The scan finds repos and folders under it and
    // marks them with the linked folder's id; `linked_exclusions` remembers the repos the user
    // removed so the next scan does not bring them back.
    "ALTER TABLE folders ADD COLUMN source_path TEXT;
     ALTER TABLE folders ADD COLUMN linked_root_id TEXT;
     ALTER TABLE folders ADD COLUMN missing INTEGER NOT NULL DEFAULT 0;
     ALTER TABLE repos ADD COLUMN linked_root_id TEXT;
     ALTER TABLE repos ADD COLUMN missing INTEGER NOT NULL DEFAULT 0;
     CREATE UNIQUE INDEX folders_source_path ON folders(source_path) WHERE source_path IS NOT NULL;
     CREATE TABLE linked_exclusions (
         root_id TEXT NOT NULL,
         path    TEXT NOT NULL,
         PRIMARY KEY (root_id, path)
     );",
    // 8: the terminal sessions the app knows, so they can be reattached or resumed after a restart
    "CREATE TABLE terminal_sessions (
         id            TEXT PRIMARY KEY,
         worktree_path TEXT NOT NULL,
         harness       TEXT,
         command       TEXT,
         shell         TEXT NOT NULL,
         resume_id     TEXT,
         started_at    INTEGER NOT NULL,
         ended_at      INTEGER,
         exit_code     INTEGER
     );
     CREATE INDEX terminal_sessions_worktree ON terminal_sessions (worktree_path);",
    // 9: orchestration runs, their tasks, and the workers (dispatches) started for the tasks
    "CREATE TABLE orchestration_runs (
         id                  TEXT PRIMARY KEY,
         coordinator_session TEXT NOT NULL,
         worktree_path       TEXT NOT NULL,
         title               TEXT NOT NULL,
         state               TEXT NOT NULL,
         budget              TEXT,
         created_at          INTEGER NOT NULL
     );
     CREATE INDEX orchestration_runs_session ON orchestration_runs (coordinator_session);
     CREATE TABLE orchestration_tasks (
         id         TEXT PRIMARY KEY,
         run_id     TEXT NOT NULL REFERENCES orchestration_runs (id),
         spec       TEXT NOT NULL,
         depends_on TEXT NOT NULL,
         state      TEXT NOT NULL,
         created_at INTEGER NOT NULL
     );
     CREATE TABLE orchestration_dispatches (
         id            TEXT PRIMARY KEY,
         run_id        TEXT NOT NULL REFERENCES orchestration_runs (id),
         task_id       TEXT NOT NULL REFERENCES orchestration_tasks (id),
         session_id    TEXT NOT NULL,
         worktree_path TEXT NOT NULL,
         harness       TEXT NOT NULL,
         state         TEXT NOT NULL,
         started_at    INTEGER NOT NULL
     );
     CREATE INDEX orchestration_dispatches_session ON orchestration_dispatches (session_id);",
    // 10: what workers send to their coordinator (reports, and what the app noticed), until it is read
    "CREATE TABLE orchestration_messages (
         id          TEXT PRIMARY KEY,
         run_id      TEXT NOT NULL REFERENCES orchestration_runs (id),
         dispatch_id TEXT,
         task_id     TEXT,
         kind        TEXT NOT NULL,
         body        TEXT NOT NULL,
         unread      INTEGER NOT NULL DEFAULT 1,
         created_at  INTEGER NOT NULL
     );
     CREATE INDEX orchestration_messages_unread ON orchestration_messages (run_id, unread);",
    // Runs keep no approval state or budget: the user's orchestration settings decide instead.
    "ALTER TABLE orchestration_runs DROP COLUMN state;
     ALTER TABLE orchestration_runs DROP COLUMN budget;",
    // The team: runs with their members, and the lines of conversation between members. Who may
    // orchestrate whom is checked by the team rules; these checks keep a row from being half-formed.
    "CREATE TABLE team_runs (
         id    TEXT PRIMARY KEY,
         title TEXT NOT NULL
     );
     CREATE TABLE team_members (
         id           TEXT PRIMARY KEY,
         run_id       TEXT NOT NULL REFERENCES team_runs (id) ON DELETE CASCADE,
         kind         TEXT NOT NULL CHECK (kind IN ('agent', 'bot')),
         role         TEXT NOT NULL,
         harness      TEXT,
         bot_id       TEXT,
         parent_id    TEXT REFERENCES team_members (id) ON DELETE SET NULL,
         orchestrator INTEGER NOT NULL DEFAULT 0,
         task         TEXT NOT NULL DEFAULT '',
         acceptance   TEXT NOT NULL DEFAULT '',
         CHECK (
             (kind = 'agent' AND harness IS NOT NULL AND bot_id IS NULL)
             OR (kind = 'bot' AND harness IS NULL AND bot_id IS NOT NULL)
         ),
         CHECK (parent_id IS NULL OR parent_id <> id)
     );
     CREATE TABLE team_pairs (
         a TEXT NOT NULL REFERENCES team_members (id) ON DELETE CASCADE,
         b TEXT NOT NULL REFERENCES team_members (id) ON DELETE CASCADE,
         PRIMARY KEY (a, b),
         CHECK (a < b)
     );",
];

#[derive(Debug, thiserror::Error)]
pub enum MigrationError {
    #[error(
        "the app database was created by a newer version (schema {found}, this build supports {supported})"
    )]
    SchemaTooNew { found: u32, supported: u32 },
    #[error("migration to schema {version} failed: {source}")]
    Failed {
        version: u32,
        #[source]
        source: rusqlite::Error,
    },
    #[error("could not read the schema version: {0}")]
    ReadVersion(#[source] rusqlite::Error),
}

pub fn migrate(connection: &mut Connection) -> Result<(), MigrationError> {
    let current: u32 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(MigrationError::ReadVersion)?;
    let supported = u32::try_from(MIGRATIONS.len()).unwrap_or(u32::MAX);
    if current > supported {
        // Downgrading would silently drop data the newer build relies on.
        return Err(MigrationError::SchemaTooNew {
            found: current,
            supported,
        });
    }

    for version in current + 1..=supported {
        let sql = MIGRATIONS[version as usize - 1];
        apply(connection, version, sql)
            .map_err(|source| MigrationError::Failed { version, source })?;
    }
    Ok(())
}

/// Each migration and its version bump commit together, so a crash mid-way
/// leaves the previous version intact.
fn apply(connection: &mut Connection, version: u32, sql: &str) -> rusqlite::Result<()> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(sql)?;
    transaction.pragma_update(None, "user_version", version)?;
    transaction.commit()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(connection: &Connection) -> u32 {
        connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read version")
    }

    #[test]
    fn fresh_database_reaches_the_latest_version() {
        let mut connection = Connection::open_in_memory().expect("open");

        migrate(&mut connection).expect("migrate");

        assert_eq!(version(&connection) as usize, MIGRATIONS.len());
    }

    #[test]
    fn migrating_twice_is_a_no_op() {
        let mut connection = Connection::open_in_memory().expect("open");
        migrate(&mut connection).expect("first");
        connection
            .execute(
                "INSERT INTO repos (id, name, path, created_at) VALUES ('a', 'a', '/a', 0)",
                [],
            )
            .expect("insert");

        migrate(&mut connection).expect("second");

        let count: u32 = connection
            .query_row("SELECT COUNT(*) FROM repos", [], |row| row.get(0))
            .expect("count");
        assert_eq!(count, 1);
    }

    #[test]
    fn every_migration_applies_cleanly_in_order() {
        // Guards against a migration that only works on top of a populated schema.
        let mut connection = Connection::open_in_memory().expect("open");
        for (index, sql) in MIGRATIONS.iter().enumerate() {
            apply(&mut connection, index as u32 + 1, sql).expect("apply");
            assert_eq!(version(&connection) as usize, index + 1);
        }
    }

    #[test]
    fn a_failing_migration_leaves_the_previous_version() {
        let mut connection = Connection::open_in_memory().expect("open");
        migrate(&mut connection).expect("migrate");
        let before = version(&connection);

        let result = apply(
            &mut connection,
            before + 1,
            "CREATE TABLE repos (x); -- already exists",
        );

        assert!(result.is_err());
        assert_eq!(version(&connection), before);
    }

    #[test]
    fn a_run_stored_with_an_approval_keeps_its_row_without_the_columns_it_lost() {
        // Before the migration that drops them, each run kept an approval state and a budget.
        let drop_columns = MIGRATIONS
            .iter()
            .position(|sql| sql.contains("DROP COLUMN state"))
            .expect("a migration drops the columns");
        let mut connection = Connection::open_in_memory().expect("open");
        for (index, sql) in MIGRATIONS.iter().enumerate().take(drop_columns) {
            apply(&mut connection, index as u32 + 1, sql).expect("apply");
        }
        connection
            .execute(
                "INSERT INTO orchestration_runs (id, coordinator_session, worktree_path, title, state, budget, created_at)
                 VALUES ('run', 'session', '/repo', 'Audit', 'approved', '{}', 0)",
                [],
            )
            .expect("insert");

        migrate(&mut connection).expect("migrate");

        let title: String = connection
            .query_row(
                "SELECT title FROM orchestration_runs WHERE id = 'run'",
                [],
                |row| row.get(0),
            )
            .expect("the run is kept");
        assert_eq!(title, "Audit");
        let columns = connection
            .prepare("SELECT name FROM pragma_table_info('orchestration_runs')")
            .expect("prepare")
            .query_map([], |row| row.get::<_, String>(0))
            .expect("query")
            .collect::<Result<Vec<String>, _>>()
            .expect("columns");
        assert!(
            !columns
                .iter()
                .any(|name| name == "state" || name == "budget")
        );
    }
}
