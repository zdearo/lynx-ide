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
    // The agents' place: the worktree an agent works in and the terminal session it runs in. Only an
    // agent has either, and a session needs a worktree; the checks keep a row from being half-formed.
    "ALTER TABLE team_members ADD COLUMN worktree TEXT
         CHECK (worktree IS NULL OR (kind = 'agent' AND worktree <> ''));
     ALTER TABLE team_members ADD COLUMN session_id TEXT
         CHECK (
             session_id IS NULL
             OR (kind = 'agent' AND session_id <> '' AND worktree IS NOT NULL)
         );",
    // The team is one per worktree: runs go, and every member names the worktree whose team it is in.
    // The team is unreleased, so its rows from the run schema are dropped, not moved. Pairs go first,
    // since they point at members.
    "DROP TABLE team_pairs;
     DROP TABLE team_members;
     DROP TABLE team_runs;
     CREATE TABLE team_members (
         id           TEXT PRIMARY KEY,
         worktree     TEXT NOT NULL CHECK (worktree <> ''),
         kind         TEXT NOT NULL CHECK (kind IN ('agent', 'bot')),
         role         TEXT NOT NULL,
         harness      TEXT,
         bot_id       TEXT,
         parent_id    TEXT REFERENCES team_members (id) ON DELETE SET NULL,
         orchestrator INTEGER NOT NULL DEFAULT 0,
         task         TEXT NOT NULL DEFAULT '',
         acceptance   TEXT NOT NULL DEFAULT '',
         session_id   TEXT CHECK (session_id IS NULL OR (kind = 'agent' AND session_id <> '')),
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
    // Whether a member's branch is open in the team tree, kept with the team so it is the same on every screen.
    "ALTER TABLE team_members ADD COLUMN expanded INTEGER NOT NULL DEFAULT 0 CHECK (expanded IN (0, 1));",
    // The team does the work: a member given a task is the worker that does it, so the run tables go. A member
    // may have no agent yet (it is not started), and it keeps its task, its state and what it waits for. SQLite
    // cannot change a CHECK in place, so the members and their lines are copied into new tables and renamed.
    "CREATE TABLE team_members_next (
         id                 TEXT PRIMARY KEY,
         worktree           TEXT NOT NULL CHECK (worktree <> ''),
         kind               TEXT NOT NULL CHECK (kind IN ('agent', 'bot')),
         role               TEXT NOT NULL,
         harness            TEXT,
         bot_id             TEXT,
         parent_id          TEXT REFERENCES team_members_next (id) ON DELETE SET NULL,
         orchestrator       INTEGER NOT NULL DEFAULT 0,
         task               TEXT NOT NULL DEFAULT '',
         acceptance         TEXT NOT NULL DEFAULT '',
         session_id         TEXT CHECK (session_id IS NULL OR (kind = 'agent' AND session_id <> '')),
         expanded           INTEGER NOT NULL DEFAULT 0 CHECK (expanded IN (0, 1)),
         task_state         TEXT CHECK (
             task_state IS NULL
             OR (kind = 'agent' AND task_state IN ('pending', 'running', 'done', 'failed', 'cancelled'))
         ),
         dispatch_state     TEXT CHECK (
             dispatch_state IS NULL
             OR (kind = 'agent' AND session_id IS NOT NULL AND dispatch_state IN ('running', 'stopped'))
         ),
         execution_worktree TEXT CHECK (
             execution_worktree IS NULL OR (kind = 'agent' AND execution_worktree <> '')
         ),
         notify_session     TEXT CHECK (notify_session IS NULL OR notify_session <> ''),
         CHECK (
             (kind = 'agent' AND bot_id IS NULL)
             OR (kind = 'bot' AND harness IS NULL AND bot_id IS NOT NULL)
         ),
         CHECK (parent_id IS NULL OR parent_id <> id)
     );
     INSERT INTO team_members_next (
         id, worktree, kind, role, harness, bot_id, parent_id, orchestrator, task, acceptance,
         session_id, expanded, task_state
     )
     SELECT id, worktree, kind, role, harness, bot_id, parent_id, orchestrator, task, acceptance,
            session_id, expanded,
            CASE WHEN kind = 'agent' AND parent_id IS NOT NULL THEN 'pending' END
     FROM team_members ORDER BY rowid;
     CREATE TABLE team_pairs_next (
         a TEXT NOT NULL REFERENCES team_members_next (id) ON DELETE CASCADE,
         b TEXT NOT NULL REFERENCES team_members_next (id) ON DELETE CASCADE,
         PRIMARY KEY (a, b),
         CHECK (a < b)
     );
     INSERT INTO team_pairs_next (a, b) SELECT a, b FROM team_pairs;
     DROP TABLE team_pairs;
     DROP TABLE team_members;
     ALTER TABLE team_members_next RENAME TO team_members;
     ALTER TABLE team_pairs_next RENAME TO team_pairs;
     CREATE TABLE team_dependencies (
         member_id  TEXT NOT NULL REFERENCES team_members (id) ON DELETE CASCADE,
         depends_on TEXT NOT NULL REFERENCES team_members (id) ON DELETE CASCADE,
         PRIMARY KEY (member_id, depends_on),
         CHECK (member_id <> depends_on)
     );
     CREATE TABLE team_messages (
         id         TEXT PRIMARY KEY,
         member_id  TEXT NOT NULL REFERENCES team_members (id) ON DELETE CASCADE,
         kind       TEXT NOT NULL CHECK (kind IN ('done', 'failed', 'noReport', 'needsYou')),
         body       TEXT NOT NULL,
         unread     INTEGER NOT NULL DEFAULT 1 CHECK (unread IN (0, 1)),
         created_at INTEGER NOT NULL
     );
     CREATE INDEX team_messages_member ON team_messages (member_id, unread);
     DROP TABLE orchestration_messages;
     DROP TABLE orchestration_dispatches;
     DROP TABLE orchestration_tasks;
     DROP TABLE orchestration_runs;",
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
    fn the_team_keeps_its_members_and_lines_when_the_runs_go() {
        // Before the last migration the team is the one the earlier migrations left, with its runs beside it.
        let last = MIGRATIONS.len() - 1;
        let mut connection = Connection::open_in_memory().expect("open");
        for (index, sql) in MIGRATIONS.iter().enumerate().take(last) {
            apply(&mut connection, index as u32 + 1, sql).expect("apply");
        }
        connection
            .execute_batch(
                "INSERT INTO team_members (id, worktree, kind, role, harness, parent_id, orchestrator, task, acceptance, session_id)
                 VALUES ('lead', '/repo', 'agent', 'Lead', 'claude', NULL, 1, '', '', 'lead-session'),
                        ('writer', '/repo', 'agent', 'Writer', 'codex', 'lead', 0, 'Escreva', 'Pronto', NULL);
                 INSERT INTO team_pairs (a, b) VALUES ('lead', 'writer');",
            )
            .expect("members and a line");

        migrate(&mut connection).expect("migrate");

        let count = |sql: &str| -> i64 {
            connection
                .query_row(sql, [], |row| row.get(0))
                .expect("count")
        };
        assert_eq!(count("SELECT COUNT(*) FROM team_members"), 2);
        assert_eq!(count("SELECT COUNT(*) FROM team_pairs"), 1);
        assert_eq!(
            count(
                "SELECT COUNT(*) FROM team_members WHERE id = 'writer' AND task_state = 'pending'"
            ),
            1,
            "a member under an orchestrator starts pending"
        );
        assert_eq!(
            count("SELECT COUNT(*) FROM team_members WHERE id = 'lead' AND task_state IS NULL"),
            1,
            "an orchestrator has no task"
        );
        assert_eq!(
            count("SELECT COUNT(*) FROM sqlite_master WHERE name LIKE 'orchestration_%'"),
            0,
            "the run tables are gone"
        );
    }

    #[test]
    fn the_team_of_runs_is_dropped_when_the_team_moves_to_worktrees() {
        // Before the migration that drops runs, each member belonged to a run and had no worktree.
        let drop_runs = MIGRATIONS
            .iter()
            .position(|sql| sql.contains("DROP TABLE team_runs"))
            .expect("a migration drops the runs");
        let mut connection = Connection::open_in_memory().expect("open");
        for (index, sql) in MIGRATIONS.iter().enumerate().take(drop_runs) {
            apply(&mut connection, index as u32 + 1, sql).expect("apply");
        }
        connection
            .execute_batch(
                "INSERT INTO team_runs (id, title) VALUES ('run', 'Auth');
                 INSERT INTO team_members (id, run_id, kind, role, harness, orchestrator)
                 VALUES ('lead', 'run', 'agent', 'Lead', 'claude', 1);
                 INSERT INTO team_members (id, run_id, kind, role, harness)
                 VALUES ('builder', 'run', 'agent', 'Builder', 'codex');
                 INSERT INTO team_pairs (a, b) VALUES ('builder', 'lead');",
            )
            .expect("insert");

        migrate(&mut connection).expect("migrate");

        // The team was never released, so its old rows are dropped rather than moved to a worktree.
        let left: u32 = connection
            .query_row("SELECT COUNT(*) FROM team_members", [], |row| row.get(0))
            .expect("count");
        let runs: u32 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name = 'team_runs'",
                [],
                |row| row.get(0),
            )
            .expect("count");
        assert_eq!((left, runs), (0, 0));
    }

    #[test]
    fn a_member_names_a_worktree_and_a_bot_never_has_a_session() {
        let mut connection = Connection::open_in_memory().expect("open");
        migrate(&mut connection).expect("migrate");
        let refused = |sql: &str| connection.execute(sql, []).is_err();

        connection
            .execute_batch(
                "INSERT INTO team_members (id, worktree, kind, role, harness, orchestrator)
                 VALUES ('lead', '/repo', 'agent', 'Lead', 'claude', 1);
                 INSERT INTO team_members (id, worktree, kind, role, bot_id, orchestrator)
                 VALUES ('docs', '/repo', 'bot', 'Docs', 'docs', 1);
                 INSERT INTO team_members (id, worktree, kind, role, harness, session_id)
                 VALUES ('builder', '/repo', 'agent', 'Builder', 'claude', 'session');",
            )
            .expect("a bot and agents in a worktree are stored");
        assert!(refused(
            "INSERT INTO team_members (id, kind, role, harness)
             VALUES ('nowhere', 'agent', 'Tester', 'claude')"
        ));
        assert!(refused(
            "INSERT INTO team_members (id, worktree, kind, role, harness)
             VALUES ('empty', '', 'agent', 'Tester', 'claude')"
        ));
        assert!(refused(
            "INSERT INTO team_members (id, worktree, kind, role, bot_id, session_id)
             VALUES ('bot', '/repo', 'bot', 'Docs', 'docs', 'session')"
        ));
        assert!(refused(
            "INSERT INTO team_members (id, worktree, kind, role, harness, session_id)
             VALUES ('blank', '/repo', 'agent', 'Tester', 'claude', '')"
        ));
    }
}
