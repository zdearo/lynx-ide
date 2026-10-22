//! The daemon's own database, `terminal-history.db` in its folder. It keeps what must outlive the
//! app's window: the sessions a command line started, and the agent events that arrived while no
//! window was open to take them.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, params};

use crate::message::{OutsideSession, PendingEvent};

const FILE: &str = "terminal-history.db";
/// Events nobody took are dropped after this many seconds.
const KEEP_EVENTS_SECONDS: i64 = 7 * 24 * 60 * 60;

const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS outside_sessions (
        id TEXT PRIMARY KEY,
        cwd TEXT NOT NULL,
        program TEXT NOT NULL,
        created_at INTEGER NOT NULL
    );
    CREATE TABLE IF NOT EXISTS events (
        seq INTEGER PRIMARY KEY AUTOINCREMENT,
        session TEXT NOT NULL,
        harness TEXT NOT NULL,
        payload TEXT NOT NULL,
        received_at INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS events_by_session ON events (session);
";

pub struct History {
    connection: Connection,
}

impl History {
    /// Opens the database in `dir`, creating it on first use, and drops events that are too old.
    pub fn open(dir: &Path) -> rusqlite::Result<Self> {
        let connection = Connection::open(dir.join(FILE))?;
        connection.busy_timeout(std::time::Duration::from_secs(2))?;
        connection.execute_batch(SCHEMA)?;
        connection.execute(
            "DELETE FROM events WHERE received_at < ?1",
            params![now_seconds() - KEEP_EVENTS_SECONDS],
        )?;
        Ok(Self { connection })
    }

    pub fn record_outside(&self, id: &str, cwd: &str, program: &str) -> rusqlite::Result<()> {
        self.connection.execute(
            "INSERT OR REPLACE INTO outside_sessions (id, cwd, program, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![id, cwd, program, now_seconds()],
        )?;
        Ok(())
    }

    pub fn outside(&self) -> rusqlite::Result<Vec<OutsideSession>> {
        let mut statement = self
            .connection
            .prepare("SELECT id, cwd, program FROM outside_sessions ORDER BY created_at, id")?;
        let rows = statement.query_map([], |row| {
            Ok(OutsideSession {
                id: row.get(0)?,
                cwd: row.get(1)?,
                program: row.get(2)?,
            })
        })?;
        rows.collect()
    }

    pub fn forget_outside(&self, id: &str) -> rusqlite::Result<()> {
        self.connection
            .execute("DELETE FROM outside_sessions WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn push_event(&self, session: &str, harness: &str, payload: &str) -> rusqlite::Result<()> {
        self.connection.execute(
            "INSERT INTO events (session, harness, payload, received_at) VALUES (?1, ?2, ?3, ?4)",
            params![session, harness, payload, now_seconds()],
        )?;
        Ok(())
    }

    /// Reads the session's events in the order they came, and removes them in the same step.
    pub fn take_events(&self, session: &str) -> rusqlite::Result<Vec<PendingEvent>> {
        let transaction = self.connection.unchecked_transaction()?;
        let events = {
            let mut statement = transaction
                .prepare("SELECT harness, payload FROM events WHERE session = ?1 ORDER BY seq")?;
            let rows = statement.query_map(params![session], |row| {
                Ok(PendingEvent {
                    harness: row.get(0)?,
                    payload: row.get(1)?,
                })
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        transaction.execute("DELETE FROM events WHERE session = ?1", params![session])?;
        transaction.commit()?;
        Ok(events)
    }
}

fn now_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
        })
}
