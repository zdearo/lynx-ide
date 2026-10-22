//! The orchestration tables of the app database: runs, their tasks, and the workers started for
//! them. Stored text is what the model parses back, so a row that no longer parses is an error.

use rusqlite::{OptionalExtension, Row, params};

use crate::agents::harness::{harness_name, harness_named};
use crate::persistence::{AppDatabase, DatabaseError};

use super::model::{Dispatch, DispatchState, Message, MessageKind, Run, Task, TaskState};

const RUN_COLUMNS: &str = "id, coordinator_session, worktree_path, title, created_at";
const TASK_COLUMNS: &str = "id, run_id, spec, depends_on, state, created_at";
const DISPATCH_COLUMNS: &str =
    "id, run_id, task_id, session_id, worktree_path, harness, state, started_at";

// shortcut: a stored value that no longer parses is reported as a database error without its own
// kind; upgrade with a typed variant when the screen needs to tell the cases apart.
fn corrupt(what: &str) -> DatabaseError {
    DatabaseError::Sqlite(rusqlite::Error::InvalidParameterName(format!(
        "unreadable orchestration {what}"
    )))
}

type RunRow = (String, String, String, String, i64);

fn run_row(row: &Row<'_>) -> rusqlite::Result<RunRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
    ))
}

fn run_from(
    (id, coordinator_session, worktree_path, title, created_at): RunRow,
) -> Result<Run, DatabaseError> {
    Ok(Run {
        id,
        coordinator_session,
        worktree_path,
        title,
        created_at,
    })
}

fn task_from(
    (id, run_id, spec, depends_on, state, created_at): (
        String,
        String,
        String,
        String,
        String,
        i64,
    ),
) -> Result<Task, DatabaseError> {
    Ok(Task {
        id,
        run_id,
        spec,
        depends_on: serde_json::from_str(&depends_on).map_err(|_| corrupt("dependencies"))?,
        state: TaskState::parse(&state).ok_or_else(|| corrupt("task state"))?,
        created_at,
    })
}

type DispatchRow = (String, String, String, String, String, String, String, i64);

fn dispatch_row(row: &Row<'_>) -> rusqlite::Result<DispatchRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
    ))
}

fn dispatch_from(
    (id, run_id, task_id, session_id, worktree_path, harness, state, started_at): DispatchRow,
) -> Result<Dispatch, DatabaseError> {
    Ok(Dispatch {
        id,
        run_id,
        task_id,
        session_id,
        worktree_path,
        harness: harness_named(&harness).ok_or_else(|| corrupt("agent"))?,
        state: DispatchState::parse(&state).ok_or_else(|| corrupt("worker state"))?,
        started_at,
    })
}

impl AppDatabase {
    pub fn insert_run(&self, run: &Run) -> Result<(), DatabaseError> {
        self.lock().execute(
            "INSERT INTO orchestration_runs (id, coordinator_session, worktree_path, title, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                run.id,
                run.coordinator_session,
                run.worktree_path,
                run.title,
                run.created_at,
            ],
        )?;
        Ok(())
    }

    /// The newest run the coordinator session started, if it started any.
    pub fn latest_run_of(&self, coordinator_session: &str) -> Result<Option<Run>, DatabaseError> {
        let row = self
            .lock()
            .query_row(
                &format!(
                    "SELECT {RUN_COLUMNS} FROM orchestration_runs WHERE coordinator_session = ?1
                     ORDER BY created_at DESC, rowid DESC LIMIT 1"
                ),
                [coordinator_session],
                run_row,
            )
            .optional()?;
        row.map(run_from).transpose()
    }

    pub fn insert_task(&self, task: &Task) -> Result<(), DatabaseError> {
        let depends_on =
            serde_json::to_string(&task.depends_on).map_err(|_| corrupt("dependencies"))?;
        self.lock().execute(
            "INSERT INTO orchestration_tasks (id, run_id, spec, depends_on, state, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                task.id,
                task.run_id,
                task.spec,
                depends_on,
                task.state.as_str(),
                task.created_at
            ],
        )?;
        Ok(())
    }

    pub fn tasks_of_run(&self, run_id: &str) -> Result<Vec<Task>, DatabaseError> {
        let connection = self.lock();
        let mut statement = connection.prepare(&format!(
            "SELECT {TASK_COLUMNS} FROM orchestration_tasks WHERE run_id = ?1 ORDER BY created_at, rowid"
        ))?;
        let rows = statement.query_map([run_id], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
            ))
        })?;
        rows.map(|row| task_from(row?)).collect()
    }

    pub fn set_task_state(&self, id: &str, state: TaskState) -> Result<(), DatabaseError> {
        self.lock().execute(
            "UPDATE orchestration_tasks SET state = ?2 WHERE id = ?1",
            params![id, state.as_str()],
        )?;
        Ok(())
    }

    pub fn insert_dispatch(&self, dispatch: &Dispatch) -> Result<(), DatabaseError> {
        self.lock().execute(
            "INSERT INTO orchestration_dispatches (id, run_id, task_id, session_id, worktree_path, harness, state, started_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                dispatch.id,
                dispatch.run_id,
                dispatch.task_id,
                dispatch.session_id,
                dispatch.worktree_path,
                harness_name(dispatch.harness),
                dispatch.state.as_str(),
                dispatch.started_at,
            ],
        )?;
        Ok(())
    }

    pub fn dispatch(&self, id: &str) -> Result<Option<Dispatch>, DatabaseError> {
        let row = self
            .lock()
            .query_row(
                &format!("SELECT {DISPATCH_COLUMNS} FROM orchestration_dispatches WHERE id = ?1"),
                [id],
                dispatch_row,
            )
            .optional()?;
        row.map(dispatch_from).transpose()
    }

    /// The worker a terminal session is, if the session is one.
    pub fn dispatch_of_session(&self, session_id: &str) -> Result<Option<Dispatch>, DatabaseError> {
        let row = self
            .lock()
            .query_row(
                &format!(
                    "SELECT {DISPATCH_COLUMNS} FROM orchestration_dispatches WHERE session_id = ?1 LIMIT 1"
                ),
                [session_id],
                dispatch_row,
            )
            .optional()?;
        row.map(dispatch_from).transpose()
    }

    pub fn dispatches_of_run(&self, run_id: &str) -> Result<Vec<Dispatch>, DatabaseError> {
        let connection = self.lock();
        let mut statement = connection.prepare(&format!(
            "SELECT {DISPATCH_COLUMNS} FROM orchestration_dispatches WHERE run_id = ?1 ORDER BY started_at, rowid"
        ))?;
        let rows = statement.query_map([run_id], dispatch_row)?;
        rows.map(|row| dispatch_from(row?)).collect()
    }

    pub fn set_dispatch_state(&self, id: &str, state: DispatchState) -> Result<(), DatabaseError> {
        self.lock().execute(
            "UPDATE orchestration_dispatches SET state = ?2 WHERE id = ?1",
            params![id, state.as_str()],
        )?;
        Ok(())
    }
}

impl AppDatabase {
    pub fn insert_message(&self, message: &Message) -> Result<(), DatabaseError> {
        self.lock().execute(
            "INSERT INTO orchestration_messages (id, run_id, dispatch_id, task_id, kind, body, unread, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7)",
            params![
                message.id,
                message.run_id,
                message.dispatch_id,
                message.task_id,
                message.kind.as_str(),
                message.body,
                message.created_at,
            ],
        )?;
        Ok(())
    }

    /// The messages nobody has read yet, oldest first. Reading them marks them read, in one step,
    /// so a message reaches the coordinator once.
    pub fn take_unread_messages(&self, run_id: &str) -> Result<Vec<Message>, DatabaseError> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let messages = {
            let mut statement = transaction.prepare(
                "SELECT id, run_id, dispatch_id, task_id, kind, body, created_at
                 FROM orchestration_messages WHERE run_id = ?1 AND unread = 1 ORDER BY rowid",
            )?;
            let rows = statement.query_map([run_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, i64>(6)?,
                ))
            })?;
            rows.map(|row| {
                let (id, run_id, dispatch_id, task_id, kind, body, created_at) = row?;
                Ok(Message {
                    id,
                    run_id,
                    dispatch_id,
                    task_id,
                    kind: MessageKind::parse(&kind).ok_or_else(|| corrupt("message kind"))?,
                    body,
                    created_at,
                })
            })
            .collect::<Result<Vec<_>, DatabaseError>>()?
        };
        transaction.execute(
            "UPDATE orchestration_messages SET unread = 0 WHERE run_id = ?1 AND unread = 1",
            [run_id],
        )?;
        transaction.commit()?;
        Ok(messages)
    }

    /// Whether the app already told the coordinator this kind of thing about a worker.
    pub fn has_message(&self, dispatch_id: &str, kind: MessageKind) -> Result<bool, DatabaseError> {
        let found: i64 = self.lock().query_row(
            "SELECT COUNT(*) FROM orchestration_messages WHERE dispatch_id = ?1 AND kind = ?2",
            params![dispatch_id, kind.as_str()],
            |row| row.get(0),
        )?;
        Ok(found > 0)
    }
}
