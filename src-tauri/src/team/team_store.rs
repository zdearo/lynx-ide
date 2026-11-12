//! The team tables of the app database. Reads take the lock; writes run inside `team_transaction`, so a
//! change with several steps lands whole or not at all.

use std::collections::HashMap;

use rusqlite::types::Type;
use rusqlite::{Connection, Row, params, params_from_iter};

use crate::agents::harness::{harness_name, harness_named};
use crate::persistence::{AppDatabase, DatabaseError};

use super::team_model::{
    DispatchState, MessageKind, TaskState, TeamExchange, TeamLastMessage, TeamMember,
    TeamMemberKind, TeamMessage, TeamPair,
};

const MEMBER_COLUMNS: &str = "id, worktree, kind, role, harness, bot_id, parent_id, orchestrator, task, acceptance, session_id, expanded, task_state, dispatch_state, execution_worktree, start_when_released";

/// A stored value the model cannot read. The table's checks keep this from happening.
fn unreadable(index: usize) -> rusqlite::Error {
    rusqlite::Error::InvalidColumnType(index, format!("column {index}"), Type::Text)
}

fn member_from(row: &Row<'_>) -> rusqlite::Result<TeamMember> {
    let kind: String = row.get(2)?;
    let harness: Option<String> = row.get(4)?;
    let task_state: Option<String> = row.get(12)?;
    let dispatch_state: Option<String> = row.get(13)?;
    Ok(TeamMember {
        id: row.get(0)?,
        worktree: row.get(1)?,
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
        task_state: task_state
            .map(|text| TaskState::parse(&text).ok_or_else(|| unreadable(12)))
            .transpose()?,
        depends_on: Vec::new(),
        dispatch_state: dispatch_state
            .map(|text| DispatchState::parse(&text).ok_or_else(|| unreadable(13)))
            .transpose()?,
        execution_worktree: row.get(14)?,
        session_id: row.get(10)?,
        expanded: row.get(11)?,
        worker_state: None,
        last_message: None,
        start_when_released: row.get(15)?,
    })
}

fn message_from(row: &Row<'_>) -> rusqlite::Result<TeamMessage> {
    let kind: String = row.get(2)?;
    Ok(TeamMessage {
        id: row.get(0)?,
        member_id: row.get(1)?,
        kind: MessageKind::parse(&kind).ok_or_else(|| unreadable(2))?,
        body: row.get(3)?,
        created_at: row.get(4)?,
    })
}

/// Reads id, kind, body, created_at, from_member_id, to_member_id, in that order.
#[cfg(test)]
fn exchange_from(row: &Row<'_>) -> rusqlite::Result<TeamExchange> {
    let kind: String = row.get(1)?;
    Ok(TeamExchange {
        id: row.get(0)?,
        kind: MessageKind::parse(&kind).ok_or_else(|| unreadable(1))?,
        body: row.get(2)?,
        created_at: row.get(3)?,
        from_member_id: row.get(4)?,
        to_member_id: row.get(5)?,
    })
}

impl AppDatabase {
    /// Every member of every worktree's team, in the order they were added, with what each waits for.
    pub fn team_members(&self) -> Result<Vec<TeamMember>, DatabaseError> {
        let connection = self.lock();
        let mut statement = connection.prepare(&format!(
            "SELECT {MEMBER_COLUMNS} FROM team_members ORDER BY rowid"
        ))?;
        let mut members = statement
            .query_map([], member_from)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut waits: HashMap<String, Vec<String>> = HashMap::new();
        let mut statement = connection
            .prepare("SELECT member_id, depends_on FROM team_dependencies ORDER BY rowid")?;
        for row in statement.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get(1)?)))? {
            let (member_id, depends_on) = row?;
            waits.entry(member_id).or_default().push(depends_on);
        }
        for member in &mut members {
            if let Some(depends_on) = waits.remove(&member.id) {
                member.depends_on = depends_on;
            }
        }
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

    /// The session a member's worker reports its messages to: the one that started it, when it started.
    pub fn notify_session_of(&self, member_id: &str) -> Result<Option<String>, DatabaseError> {
        let session = self
            .lock()
            .query_row(
                "SELECT notify_session FROM team_members WHERE id = ?1",
                [member_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .map_err(DatabaseError::from)?;
        Ok(session)
    }

    /// The messages a member has had, oldest first. Reading them does not mark them read.
    pub fn messages_of(&self, member_id: &str) -> Result<Vec<TeamMessage>, DatabaseError> {
        let connection = self.lock();
        let mut statement = connection.prepare(
            "SELECT id, member_id, kind, body, created_at FROM team_messages
             WHERE member_id = ?1 ORDER BY created_at, rowid",
        )?;
        let rows = statement.query_map([member_id], message_from)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// The newest message kept for each member that has one, as the screen shows it.
    pub fn last_messages(&self) -> Result<HashMap<String, TeamLastMessage>, DatabaseError> {
        let connection = self.lock();
        let mut statement = connection.prepare(
            "SELECT member_id, kind, body, created_at FROM (
                 SELECT member_id, kind, body, created_at,
                        ROW_NUMBER() OVER (PARTITION BY member_id ORDER BY created_at DESC, rowid DESC) AS newest
                 FROM team_messages
             ) WHERE newest = 1",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                TeamLastMessage {
                    kind: row.get(1)?,
                    body: row.get(2)?,
                    created_at: row.get(3)?,
                },
            ))
        })?;
        Ok(rows.collect::<rusqlite::Result<HashMap<_, _>>>()?)
    }

    /// The questions and answers a member sent or received, oldest first. Reading them does not mark them read.
    /// Only the tests read the lines back: the app shows the asker, through the messages.
    #[cfg(test)]
    pub fn exchanges_of(&self, member_id: &str) -> Result<Vec<TeamExchange>, DatabaseError> {
        let connection = self.lock();
        let mut statement = connection.prepare(
            "SELECT id, kind, body, created_at, from_member_id, to_member_id FROM team_messages
             WHERE from_member_id = ?1 OR to_member_id = ?1 ORDER BY created_at, rowid",
        )?;
        let rows = statement.query_map([member_id], exchange_from)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
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
pub(super) fn insert_member(
    connection: &Connection,
    member: &TeamMember,
) -> Result<(), DatabaseError> {
    connection.execute(
        &format!(
            "INSERT INTO team_members ({MEMBER_COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)"
        ),
        params![
            member.id,
            member.worktree,
            member.kind.as_str(),
            member.role,
            member.harness.map(harness_name),
            member.bot_id,
            member.parent_id,
            member.orchestrator,
            member.task,
            member.acceptance,
            member.session_id,
            member.expanded,
            member.task_state.map(TaskState::as_str),
            member.dispatch_state.map(DispatchState::as_str),
            member.execution_worktree,
            member.start_when_released,
        ],
    )?;
    Ok(())
}

/// Writes whether a member's branch is open. Nothing else of the member moves.
pub(super) fn update_expanded(
    connection: &Connection,
    member_id: &str,
    expanded: bool,
) -> Result<(), DatabaseError> {
    connection.execute(
        "UPDATE team_members SET expanded = ?2 WHERE id = ?1",
        params![member_id, expanded],
    )?;
    Ok(())
}

/// Writes the terminal session of an agent. The worktree stays, and so do the other fields.
pub(super) fn update_session(
    connection: &Connection,
    member: &TeamMember,
) -> Result<(), DatabaseError> {
    connection.execute(
        "UPDATE team_members SET session_id = ?2 WHERE id = ?1",
        params![member.id, member.session_id],
    )?;
    Ok(())
}

/// Writes the fields a change can move. The kind, the worktree and the bot never change.
pub(super) fn update_member(
    connection: &Connection,
    member: &TeamMember,
) -> Result<(), DatabaseError> {
    connection.execute(
        "UPDATE team_members
         SET role = ?2, parent_id = ?3, orchestrator = ?4, task = ?5, acceptance = ?6, task_state = ?7
         WHERE id = ?1",
        params![
            member.id,
            member.role,
            member.parent_id,
            member.orchestrator,
            member.task,
            member.acceptance,
            member.task_state.map(TaskState::as_str),
        ],
    )?;
    Ok(())
}

/// Gives the member a worker: its agent, its session, the worktree it works in, and a running task that reports
/// to `notify_session`. Its team worktree and place in the tree stay.
pub(super) fn start_worker_row(
    connection: &Connection,
    member_id: &str,
    harness: &str,
    session_id: &str,
    execution_worktree: &str,
    notify_session: &str,
) -> Result<(), DatabaseError> {
    connection.execute(
        "UPDATE team_members
         SET harness = ?2, session_id = ?3, execution_worktree = ?4, notify_session = ?5,
             task_state = 'running', dispatch_state = 'running'
         WHERE id = ?1",
        params![
            member_id,
            harness,
            session_id,
            execution_worktree,
            notify_session
        ],
    )?;
    Ok(())
}

pub(super) fn set_task_state_row(
    connection: &Connection,
    member_id: &str,
    state: TaskState,
) -> Result<(), DatabaseError> {
    connection.execute(
        "UPDATE team_members SET task_state = ?2 WHERE id = ?1",
        params![member_id, state.as_str()],
    )?;
    Ok(())
}

pub(super) fn set_dispatch_state_row(
    connection: &Connection,
    member_id: &str,
    state: DispatchState,
) -> Result<(), DatabaseError> {
    connection.execute(
        "UPDATE team_members SET dispatch_state = ?2 WHERE id = ?1",
        params![member_id, state.as_str()],
    )?;
    Ok(())
}

/// Replaces what a member waits for. The ids are checked by the rules before they get here.
pub(super) fn replace_dependencies(
    connection: &Connection,
    member_id: &str,
    depends_on: &[String],
) -> Result<(), DatabaseError> {
    connection.execute(
        "DELETE FROM team_dependencies WHERE member_id = ?1",
        [member_id],
    )?;
    for dependency in depends_on {
        connection.execute(
            "INSERT INTO team_dependencies (member_id, depends_on) VALUES (?1, ?2)",
            params![member_id, dependency],
        )?;
    }
    Ok(())
}

pub(super) fn insert_message(
    connection: &Connection,
    message: &TeamMessage,
) -> Result<(), DatabaseError> {
    connection.execute(
        "INSERT INTO team_messages (id, member_id, kind, body, unread, created_at)
         VALUES (?1, ?2, ?3, ?4, 1, ?5)",
        params![
            message.id,
            message.member_id,
            message.kind.as_str(),
            message.body,
            message.created_at
        ],
    )?;
    Ok(())
}

/// Keeps a question or an answer, and carries both members. It is filed under the asker, who sent the question and
/// receives its answer, so the asker's orchestrator hears both at its next check, whoever answered.
pub(super) fn insert_exchange(
    connection: &Connection,
    exchange: &TeamExchange,
) -> Result<(), DatabaseError> {
    let asker = if exchange.kind == MessageKind::Answer {
        &exchange.to_member_id
    } else {
        &exchange.from_member_id
    };
    connection.execute(
        "INSERT INTO team_messages (id, member_id, kind, body, unread, created_at, from_member_id, to_member_id)
         VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6, ?7)",
        params![
            exchange.id,
            asker,
            exchange.kind.as_str(),
            exchange.body,
            exchange.created_at,
            exchange.from_member_id,
            exchange.to_member_id
        ],
    )?;
    Ok(())
}

/// Marks one message read, so the next check does not say it again.
pub(super) fn mark_message_read(
    connection: &Connection,
    message_id: &str,
) -> Result<(), DatabaseError> {
    connection.execute(
        "UPDATE team_messages SET unread = 0 WHERE id = ?1",
        [message_id],
    )?;
    Ok(())
}

/// The messages nobody has read yet from the members under any of `parent_ids`, oldest first. Taking them marks
/// them read, so each reaches its orchestrator once.
pub(super) fn take_unread_messages(
    connection: &Connection,
    parent_ids: &[String],
) -> Result<Vec<TeamMessage>, DatabaseError> {
    if parent_ids.is_empty() {
        return Ok(Vec::new());
    }
    let holes = vec!["?"; parent_ids.len()].join(", ");
    let mut statement = connection.prepare(&format!(
        "SELECT m.id, m.member_id, m.kind, m.body, m.created_at
         FROM team_messages m JOIN team_members t ON t.id = m.member_id
         WHERE m.unread = 1 AND t.parent_id IN ({holes})
         ORDER BY m.created_at, m.rowid"
    ))?;
    let messages = statement
        .query_map(params_from_iter(parent_ids.iter()), message_from)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);
    connection.execute(
        &format!(
            "UPDATE team_messages SET unread = 0
             WHERE unread = 1 AND member_id IN (SELECT id FROM team_members WHERE parent_id IN ({holes}))"
        ),
        params_from_iter(parent_ids.iter()),
    )?;
    Ok(messages)
}

/// Empties the task of every member under `parent_id`. Used before the parent goes, so they keep no task
/// once they answer to no one.
pub(super) fn clear_tasks_of(
    connection: &Connection,
    parent_id: &str,
) -> Result<(), DatabaseError> {
    connection.execute(
        "UPDATE team_members SET task = '', acceptance = '', task_state = NULL WHERE parent_id = ?1",
        [parent_id],
    )?;
    Ok(())
}

/// Removing a member removes its lines, what it waits for and its messages (cascade). The members under it
/// answer to no one (set null).
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
