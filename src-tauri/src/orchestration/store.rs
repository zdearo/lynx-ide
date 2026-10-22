//! The orchestration tables of the app database: runs, their tasks, and the workers started for
//! them. Stored text is what the model parses back, so a row that no longer parses is an error.

use rusqlite::{OptionalExtension, Row, params};

use crate::agents::harness::{harness_name, harness_named};
use crate::persistence::{AppDatabase, DatabaseError};

use super::model::{Budget, Dispatch, DispatchState, Run, RunState, Task, TaskState};

const RUN_COLUMNS: &str =
    "id, coordinator_session, worktree_path, title, state, budget, created_at";
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

type RunRow = (String, String, String, String, String, Option<String>, i64);

fn run_row(row: &Row<'_>) -> rusqlite::Result<RunRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
    ))
}

fn run_from(
    (id, coordinator_session, worktree_path, title, state, budget, created_at): RunRow,
) -> Result<Run, DatabaseError> {
    let budget = match budget {
        Some(text) => Some(serde_json::from_str::<Budget>(&text).map_err(|_| corrupt("budget"))?),
        None => None,
    };
    Ok(Run {
        id,
        coordinator_session,
        worktree_path,
        title,
        state: RunState::parse(&state).ok_or_else(|| corrupt("run state"))?,
        budget,
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
        let budget = match &run.budget {
            Some(budget) => Some(serde_json::to_string(budget).map_err(|_| corrupt("budget"))?),
            None => None,
        };
        self.lock().execute(
            "INSERT INTO orchestration_runs (id, coordinator_session, worktree_path, title, state, budget, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                run.id,
                run.coordinator_session,
                run.worktree_path,
                run.title,
                run.state.as_str(),
                budget,
                run.created_at,
            ],
        )?;
        Ok(())
    }

    pub fn run(&self, id: &str) -> Result<Option<Run>, DatabaseError> {
        let row = self
            .lock()
            .query_row(
                &format!("SELECT {RUN_COLUMNS} FROM orchestration_runs WHERE id = ?1"),
                [id],
                run_row,
            )
            .optional()?;
        row.map(run_from).transpose()
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

    /// Approves a run that is waiting. Returns false when it was not waiting, so a second approval
    /// cannot replace the first budget.
    pub fn approve_run(&self, id: &str, budget: &Budget) -> Result<bool, DatabaseError> {
        let text = serde_json::to_string(budget).map_err(|_| corrupt("budget"))?;
        let changed = self.lock().execute(
            "UPDATE orchestration_runs SET state = ?2, budget = ?3 WHERE id = ?1 AND state = ?4",
            params![
                id,
                RunState::Approved.as_str(),
                text,
                RunState::AwaitingApproval.as_str()
            ],
        )?;
        Ok(changed == 1)
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
