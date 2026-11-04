//! The team's changes. Each one reads the team, checks the rules, and writes in one transaction. A lock
//! lets one change at a time, so two requests cannot both pass a check that the other one breaks.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::persistence::AppDatabase;

use super::team_error::TeamError;
use super::team_model::{NewTeamMember, TeamMember, TeamMemberKind, TeamPair, TeamSnapshot};
use super::team_rules::{
    check_agent_only, check_bot_id, check_bot_join, check_has_orchestrator, check_orchestrator_off,
    check_pair, check_parent, check_role_change, clean_role, clean_session_id, clean_text,
    clean_worktree, member_of,
};
use super::team_store::{
    clear_tasks_of, insert_member, insert_pair, remove_member, remove_pair, update_expanded,
    update_member, update_session,
};

pub struct TeamService {
    database: Arc<AppDatabase>,
    /// Whether a bot id names a bot that exists. The app gives it the bots folder check.
    bot_exists: Box<dyn Fn(&str) -> bool + Send + Sync>,
    /// Held for each read and change, so a change never checks a team another change is writing.
    turns: Mutex<()>,
}

impl TeamService {
    pub fn new(
        database: Arc<AppDatabase>,
        bot_exists: Box<dyn Fn(&str) -> bool + Send + Sync>,
    ) -> Self {
        Self {
            database,
            bot_exists,
            turns: Mutex::new(()),
        }
    }

    fn turn(&self) -> MutexGuard<'_, ()> {
        self.turns.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A bot joins a team only while it exists. Asked under the turn, so a bot deleted meanwhile is refused.
    fn check_bot(&self, bot_id: &str) -> Result<(), TeamError> {
        check_bot_id(bot_id)?;
        if (self.bot_exists)(bot_id) {
            Ok(())
        } else {
            Err(TeamError::UnknownBot)
        }
    }

    /// Every worktree's members and lines, read in one turn so they belong together.
    pub fn snapshot(&self) -> Result<TeamSnapshot, TeamError> {
        let _turn = self.turn();
        Ok(TeamSnapshot {
            members: self.database.team_members()?,
            pairs: self.database.team_pairs()?,
        })
    }

    /// Adds a member to its worktree's team. An agent may start under an orchestrator of that worktree; without
    /// one it is the root of a new tree and gets no task. A bot joins as an orchestrator.
    pub fn add_member(&self, input: NewTeamMember) -> Result<TeamMember, TeamError> {
        let _turn = self.turn();
        let members = self.database.team_members()?;
        let mut member = match input {
            NewTeamMember::Agent {
                role,
                harness,
                parent_id,
                orchestrator,
                task,
                acceptance,
                worktree,
                session_id,
            } => {
                let mut member = TeamMember {
                    harness: Some(harness),
                    orchestrator,
                    session_id: session_id.as_deref().map(clean_session_id).transpose()?,
                    ..blank_member(
                        clean_worktree(&worktree)?,
                        TeamMemberKind::Agent,
                        clean_role(&role)?,
                    )
                };
                // A member with no orchestrator keeps no task, so the task is only read with a parent.
                if let Some(parent_id) = parent_id {
                    check_parent(&members, &member, &parent_id)?;
                    member.task = clean_text(&task)?;
                    member.acceptance = clean_text(&acceptance)?;
                    member.parent_id = Some(parent_id);
                }
                member
            }
            NewTeamMember::Bot {
                bot_id,
                role,
                worktree,
            } => {
                let worktree = clean_worktree(&worktree)?;
                self.check_bot(&bot_id)?;
                check_bot_join(&members, &worktree, &bot_id)?;
                TeamMember {
                    bot_id: Some(bot_id),
                    orchestrator: true,
                    ..blank_member(worktree, TeamMemberKind::Bot, clean_role(&role)?)
                }
            }
        };
        // A root orchestrator opens its branch; anything else starts closed.
        member.expanded = member.orchestrator && member.parent_id.is_none();
        self.database
            .team_transaction(|connection| insert_member(connection, &member))?;
        Ok(member)
    }

    /// Opens or closes a member's branch in the team tree. The screen reads it back from the snapshot.
    pub fn set_expanded(&self, member_id: &str, expanded: bool) -> Result<(), TeamError> {
        let _turn = self.turn();
        member_of(&self.database.team_members()?, member_id)?;
        self.database
            .team_transaction(|connection| update_expanded(connection, member_id, expanded))?;
        Ok(())
    }

    /// Puts a member under an orchestrator, or under no one. Leaving the orchestrator clears the member's task,
    /// and drops a line between the two: they already talk through the tree.
    pub fn set_parent(&self, member_id: &str, parent_id: Option<&str>) -> Result<(), TeamError> {
        let _turn = self.turn();
        let members = self.database.team_members()?;
        let mut member = member_of(&members, member_id)?.clone();
        let tree_line = match parent_id {
            Some(parent_id) => {
                check_parent(&members, &member, parent_id)?;
                member.parent_id = Some(parent_id.to_owned());
                Some(TeamPair::between(member_id, parent_id))
            }
            None => {
                member.parent_id = None;
                member.task.clear();
                member.acceptance.clear();
                None
            }
        };
        self.database.team_transaction(|connection| {
            update_member(connection, &member)?;
            match &tree_line {
                Some(pair) => remove_pair(connection, pair),
                None => Ok(()),
            }
        })?;
        Ok(())
    }

    /// Makes a member an orchestrator, or takes that away. A member with members under it keeps it, and a
    /// bot always keeps it.
    pub fn set_orchestrator(&self, member_id: &str, on: bool) -> Result<(), TeamError> {
        let _turn = self.turn();
        let members = self.database.team_members()?;
        let mut member = member_of(&members, member_id)?.clone();
        if !on {
            check_orchestrator_off(&members, member_id)?;
        }
        member.orchestrator = on;
        self.database
            .team_transaction(|connection| update_member(connection, &member))?;
        Ok(())
    }

    /// Gives an agent a new role. A bot keeps its name.
    pub fn set_role(&self, member_id: &str, role: &str) -> Result<(), TeamError> {
        let _turn = self.turn();
        let mut member = member_of(&self.database.team_members()?, member_id)?.clone();
        check_role_change(&member)?;
        member.role = clean_role(role)?;
        self.database
            .team_transaction(|connection| update_member(connection, &member))?;
        Ok(())
    }

    /// Sets the task and the "done when" line. Only a member with an orchestrator above it has them.
    pub fn set_task(&self, member_id: &str, task: &str, acceptance: &str) -> Result<(), TeamError> {
        let _turn = self.turn();
        let mut member = member_of(&self.database.team_members()?, member_id)?.clone();
        check_has_orchestrator(&member)?;
        member.task = clean_text(task)?;
        member.acceptance = clean_text(acceptance)?;
        self.database
            .team_transaction(|connection| update_member(connection, &member))?;
        Ok(())
    }

    /// Attaches a terminal session to an agent, or detaches it with None. The worktree stays, so a session
    /// restored later goes back there.
    pub fn set_session(&self, member_id: &str, session_id: Option<&str>) -> Result<(), TeamError> {
        let _turn = self.turn();
        let mut member = member_of(&self.database.team_members()?, member_id)?.clone();
        check_agent_only(&member)?;
        member.session_id = session_id.map(clean_session_id).transpose()?;
        self.database
            .team_transaction(|connection| update_session(connection, &member))?;
        Ok(())
    }

    /// Removes a member. Its lines go with it. The members it orchestrated answer to no one, and lose their task.
    /// Returns the terminal session the member ran in, so the screen can close it.
    pub fn dismiss_member(&self, member_id: &str) -> Result<Option<String>, TeamError> {
        let _turn = self.turn();
        let members = self.database.team_members()?;
        let session_id = member_of(&members, member_id)?.session_id.clone();
        self.database.team_transaction(|connection| {
            clear_tasks_of(connection, member_id)?;
            remove_member(connection, member_id)
        })?;
        Ok(session_id)
    }

    /// Removes a bot from every worktree's team: its members, the members under them at any depth, and their
    /// lines. Nothing refuses it, since the person already deleted the bot. Returns how many members went.
    pub fn remove_bot(&self, bot_id: &str) -> Result<usize, TeamError> {
        let _turn = self.turn();
        let gone = bot_and_its_members(&self.database.team_members()?, bot_id);
        self.remove_members(&gone)
    }

    /// Removes every bot that `bot_exists` no longer accepts, each with the same cascade as `remove_bot`, in one
    /// transaction. The app calls it once at startup, to clear a bot deleted before its team lines went with it.
    /// Returns how many members went.
    pub fn remove_missing_bots(
        &self,
        bot_exists: impl Fn(&str) -> bool,
    ) -> Result<usize, TeamError> {
        let _turn = self.turn();
        let members = self.database.team_members()?;
        let mut gone: Vec<String> = members
            .iter()
            .filter_map(|member| member.bot_id.as_deref())
            .filter(|bot_id| !bot_exists(bot_id))
            .flat_map(|bot_id| bot_and_its_members(&members, bot_id))
            .collect();
        gone.sort_unstable();
        gone.dedup();
        self.remove_members(&gone)
    }

    /// Removes the members in one transaction, and returns how many were listed.
    fn remove_members(&self, gone: &[String]) -> Result<usize, TeamError> {
        self.database.team_transaction(|connection| {
            for id in gone {
                remove_member(connection, id)?;
            }
            Ok(())
        })?;
        Ok(gone.len())
    }

    /// Draws or removes the line between two members. Removing a line is always allowed.
    pub fn set_linked(&self, a: &str, b: &str, linked: bool) -> Result<(), TeamError> {
        let _turn = self.turn();
        let pair = TeamPair::between(a, b);
        if linked {
            let members = self.database.team_members()?;
            let pairs = self.database.team_pairs()?;
            check_pair(&members, &pairs, a, b)?;
            self.database
                .team_transaction(|connection| insert_pair(connection, &pair))?;
        } else {
            self.database
                .team_transaction(|connection| remove_pair(connection, &pair))?;
        }
        Ok(())
    }
}

/// A new member under no one, with no task and not an orchestrator. The caller sets the rest.
fn blank_member(worktree: String, kind: TeamMemberKind, role: String) -> TeamMember {
    TeamMember {
        id: new_id(),
        worktree,
        kind,
        role,
        harness: None,
        bot_id: None,
        parent_id: None,
        orchestrator: false,
        task: String::new(),
        acceptance: String::new(),
        session_id: None,
        expanded: false,
    }
}

/// The bot's members in every worktree, then every member under them at any depth. Each id is listed once, so a
/// loop in the stored parents still ends.
fn bot_and_its_members(members: &[TeamMember], bot_id: &str) -> Vec<String> {
    let mut found: Vec<String> = members
        .iter()
        .filter(|member| {
            member.kind == TeamMemberKind::Bot && member.bot_id.as_deref() == Some(bot_id)
        })
        .map(|member| member.id.clone())
        .collect();
    let mut next = 0;
    while let Some(parent) = found.get(next).cloned() {
        next += 1;
        for member in members {
            if member.parent_id.as_deref() == Some(parent.as_str()) && !found.contains(&member.id) {
                found.push(member.id.clone());
            }
        }
    }
    found
}

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
