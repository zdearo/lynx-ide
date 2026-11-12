//! What a team keeps besides its members: role presets (a name and the prompt of that role, for every team)
//! and the notes each team shares, by worktree.

use rusqlite::{OptionalExtension, params};

use crate::persistence::{AppDatabase, DatabaseError};

use super::team_model::TeamNote;

impl AppDatabase {
    /// Every role preset as (name, prompt), by name.
    pub fn team_roles(&self) -> Result<Vec<(String, String)>, DatabaseError> {
        let connection = self.lock();
        let mut statement =
            connection.prepare("SELECT name, prompt FROM team_roles ORDER BY name")?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// The prompt of the preset called `name`, ignoring case.
    pub fn team_role_prompt(&self, name: &str) -> Result<Option<String>, DatabaseError> {
        Ok(self
            .lock()
            .query_row(
                "SELECT prompt FROM team_roles WHERE name = ?1",
                [name],
                |row| row.get(0),
            )
            .optional()?)
    }

    /// Creates the preset, or replaces the prompt of the one with that name.
    pub fn save_team_role(&self, name: &str, prompt: &str) -> Result<(), DatabaseError> {
        self.lock().execute(
            "INSERT INTO team_roles (name, prompt) VALUES (?1, ?2)
             ON CONFLICT (name) DO UPDATE SET prompt = excluded.prompt",
            params![name, prompt],
        )?;
        Ok(())
    }

    /// Removes the preset. Returns whether there was one.
    pub fn delete_team_role(&self, name: &str) -> Result<bool, DatabaseError> {
        Ok(self
            .lock()
            .execute("DELETE FROM team_roles WHERE name = ?1", [name])?
            > 0)
    }

    /// Every note of every worktree's team, in worktree and name order.
    pub fn team_notes(&self) -> Result<Vec<TeamNote>, DatabaseError> {
        let connection = self.lock();
        let mut statement = connection
            .prepare("SELECT worktree, name, content FROM team_notes ORDER BY worktree, name")?;
        let rows = statement.query_map([], |row| {
            Ok(TeamNote {
                worktree: row.get(0)?,
                name: row.get(1)?,
                body: row.get(2)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// The names of the notes a worktree's team shares, by name.
    pub fn team_note_names(&self, worktree: &str) -> Result<Vec<String>, DatabaseError> {
        let connection = self.lock();
        let mut statement =
            connection.prepare("SELECT name FROM team_notes WHERE worktree = ?1 ORDER BY name")?;
        let rows = statement.query_map([worktree], |row| row.get(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn team_note(&self, worktree: &str, name: &str) -> Result<Option<String>, DatabaseError> {
        Ok(self
            .lock()
            .query_row(
                "SELECT content FROM team_notes WHERE worktree = ?1 AND name = ?2",
                params![worktree, name],
                |row| row.get(0),
            )
            .optional()?)
    }

    /// Creates the note, or replaces its content.
    pub fn save_team_note(
        &self,
        worktree: &str,
        name: &str,
        content: &str,
    ) -> Result<(), DatabaseError> {
        self.lock().execute(
            "INSERT INTO team_notes (worktree, name, content) VALUES (?1, ?2, ?3)
             ON CONFLICT (worktree, name) DO UPDATE SET content = excluded.content",
            params![worktree, name, content],
        )?;
        Ok(())
    }

    /// Removes the note. Returns whether there was one.
    pub fn delete_team_note(&self, worktree: &str, name: &str) -> Result<bool, DatabaseError> {
        Ok(self.lock().execute(
            "DELETE FROM team_notes WHERE worktree = ?1 AND name = ?2",
            params![worktree, name],
        )? > 0)
    }
}
