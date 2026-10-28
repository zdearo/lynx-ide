//! The team tables of the app database. Reads take the lock; writes run inside `team_transaction`, so a
//! change with several steps lands whole or not at all.

use rusqlite::types::Type;
use rusqlite::{Connection, Row, params};

use crate::agents::harness::{harness_name, harness_named};
use crate::persistence::{AppDatabase, DatabaseError};

use super::team_model::{TeamMember, TeamMemberKind, TeamPair, TeamRun};

const MEMBER_COLUMNS: &str =
    "id, run_id, kind, role, harness, bot_id, parent_id, orchestrator, task, acceptance";

/// A stored value the model cannot read. The table's checks keep this from happening.
fn unreadable(index: usize) -> rusqlite::Error {
    rusqlite::Error::InvalidColumnType(index, format!("column {index}"), Type::Text)
}

fn member_from(row: &Row<'_>) -> rusqlite::Result<TeamMember> {
    let kind: String = row.get(2)?;
    let harness: Option<String> = row.get(4)?;
    Ok(TeamMember {
        id: row.get(0)?,
        run_id: row.get(1)?,
        kind: TeamMemberKind::parse(&kind).ok_or_else(|| unreadable(2))?,
        role: row.get(3)?,
        harness: harness
            .map(|name| harness_named(&name).ok_or_else(|| unreadable(4)))
            .transpose()?,
        bot_id: row.get(5)?,
        parent_id: row.get(6)?,
        orchestrator: row.get(7)?,
        task: row.get(8)?,
        acceptance: row.get(9)?,
    })
}

impl AppDatabase {
    /// The runs, in the order they were made.
    pub fn team_runs(&self) -> Result<Vec<TeamRun>, DatabaseError> {
        let connection = self.lock();
        let mut statement = connection.prepare("SELECT id, title FROM team_runs ORDER BY rowid")?;
        let runs = statement
            .query_map([], |row| {
                Ok(TeamRun {
                    id: row.get(0)?,
                    title: row.get(1)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(runs)
    }

    /// Every member of every run, in the order they were added.
    pub fn team_members(&self) -> Result<Vec<TeamMember>, DatabaseError> {
        let connection = self.lock();
        let mut statement = connection.prepare(&format!(
            "SELECT {MEMBER_COLUMNS} FROM team_members ORDER BY rowid"
        ))?;
        let members = statement
            .query_map([], member_from)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(members)
    }

    /// Every line between two members, each stored once with the lower id first.
    pub fn team_pairs(&self) -> Result<Vec<TeamPair>, DatabaseError> {
        let connection = self.lock();
        let mut statement = connection.prepare("SELECT a, b FROM team_pairs ORDER BY rowid")?;
        let pairs = statement
            .query_map([], |row| {
                Ok(TeamPair {
                    a: row.get(0)?,
                    b: row.get(1)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(pairs)
    }

    /// Runs `change` inside one transaction: its writes all land, or, when it fails, none does.
    pub fn team_transaction<T>(
        &self,
        change: impl FnOnce(&Connection) -> Result<T, DatabaseError>,
    ) -> Result<T, DatabaseError> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let value = change(&transaction)?;
        transaction.commit()?;
        Ok(value)
    }
}

/// Writes for `team_transaction`. They take the connection the transaction gives them.
pub(super) fn insert_run(connection: &Connection, run: &TeamRun) -> Result<(), DatabaseError> {
    connection.execute(
        "INSERT INTO team_runs (id, title) VALUES (?1, ?2)",
        params![run.id, run.title],
    )?;
    Ok(())
}

/// Removing a run removes its members and their lines too (the tables cascade).
pub(super) fn remove_run(connection: &Connection, id: &str) -> Result<(), DatabaseError> {
    connection.execute("DELETE FROM team_runs WHERE id = ?1", [id])?;
    Ok(())
}

pub(super) fn insert_member(
    connection: &Connection,
    member: &TeamMember,
) -> Result<(), DatabaseError> {
    connection.execute(
        &format!(
            "INSERT INTO team_members ({MEMBER_COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"
        ),
        params![
            member.id,
            member.run_id,
            member.kind.as_str(),
            member.role,
            member.harness.map(harness_name),
            member.bot_id,
            member.parent_id,
            member.orchestrator,
            member.task,
            member.acceptance,
        ],
    )?;
    Ok(())
}

/// Writes the fields a change can move. The kind, the run and the bot never change.
pub(super) fn update_member(
    connection: &Connection,
    member: &TeamMember,
) -> Result<(), DatabaseError> {
    connection.execute(
        "UPDATE team_members
         SET role = ?2, parent_id = ?3, orchestrator = ?4, task = ?5, acceptance = ?6
         WHERE id = ?1",
        params![
            member.id,
            member.role,
            member.parent_id,
            member.orchestrator,
            member.task,
            member.acceptance,
        ],
    )?;
    Ok(())
}

/// Empties the task of every member under `parent_id`. Used before the parent goes, so they keep no task
/// once they answer to no one.
pub(super) fn clear_tasks_of(
    connection: &Connection,
    parent_id: &str,
) -> Result<(), DatabaseError> {
    connection.execute(
        "UPDATE team_members SET task = '', acceptance = '' WHERE parent_id = ?1",
        [parent_id],
    )?;
    Ok(())
}

/// Removing a member removes its lines too (cascade), and the members under it answer to no one (set null).
pub(super) fn remove_member(connection: &Connection, id: &str) -> Result<(), DatabaseError> {
    connection.execute("DELETE FROM team_members WHERE id = ?1", [id])?;
    Ok(())
}

pub(super) fn insert_pair(connection: &Connection, pair: &TeamPair) -> Result<(), DatabaseError> {
    connection.execute(
        "INSERT INTO team_pairs (a, b) VALUES (?1, ?2)",
        params![pair.a, pair.b],
    )?;
    Ok(())
}

/// Removes a line if there is one; a line that is not there is not an error.
pub(super) fn remove_pair(connection: &Connection, pair: &TeamPair) -> Result<(), DatabaseError> {
    connection.execute(
        "DELETE FROM team_pairs WHERE a = ?1 AND b = ?2",
        params![pair.a, pair.b],
    )?;
    Ok(())
}
