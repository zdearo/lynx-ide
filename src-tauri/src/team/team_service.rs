//! The team's changes. Each one reads the team, checks the rules, and writes in one transaction. A lock
//! lets one change at a time, so two requests cannot both pass a check that the other one breaks.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::agents::harness::HarnessId;
use crate::persistence::AppDatabase;

use super::team_error::TeamError;
use super::team_model::{
    NewTeamMember, TeamMember, TeamMemberKind, TeamPair, TeamRun, TeamSnapshot,
};
use super::team_rules::{
    check_bot_id, check_bot_join, check_has_orchestrator, check_orchestrator_off, check_pair,
    check_parent, check_role_change, clean_role, clean_text, clean_title, member_of,
};
use super::team_store::{
    clear_tasks_of, insert_member, insert_pair, insert_run, remove_member, remove_pair, remove_run,
    update_member,
};

const LEAD_ROLE: &str = "Lead";

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

    /// The run, or the error that says there is none. Callers hold the turn.
    fn run_exists(&self, run_id: &str) -> Result<(), TeamError> {
        if self
            .database
            .team_runs()?
            .iter()
            .any(|run| run.id == run_id)
        {
            return Ok(());
        }
        Err(TeamError::RunNotFound(run_id.to_owned()))
    }

    /// The whole team, read in one turn so the runs, members and lines belong together.
    pub fn snapshot(&self) -> Result<TeamSnapshot, TeamError> {
        let _turn = self.turn();
        Ok(TeamSnapshot {
            runs: self.database.team_runs()?,
            members: self.database.team_members()?,
            pairs: self.database.team_pairs()?,
        })
    }

    /// Starts a run with its orchestrator at the top: a Claude agent called Lead, or the bot `bot_id`.
    pub fn create_run(&self, title: &str, bot_id: Option<&str>) -> Result<TeamRun, TeamError> {
        let _turn = self.turn();
        let run = TeamRun {
            id: new_id(),
            title: clean_title(title)?,
        };
        let lead = match bot_id {
            Some(bot_id) => {
                self.check_bot(bot_id)?;
                TeamMember {
                    bot_id: Some(bot_id.to_owned()),
                    orchestrator: true,
                    ..blank_member(&run.id, TeamMemberKind::Bot, LEAD_ROLE.to_owned())
                }
            }
            None => TeamMember {
                harness: Some(HarnessId::Claude),
                orchestrator: true,
                ..blank_member(&run.id, TeamMemberKind::Agent, LEAD_ROLE.to_owned())
            },
        };
        self.database.team_transaction(|connection| {
            insert_run(connection, &run)?;
            insert_member(connection, &lead)
        })?;
        Ok(run)
    }

    /// Removes a run with its members and their lines. The others keep theirs.
    pub fn delete_run(&self, run_id: &str) -> Result<(), TeamError> {
        let _turn = self.turn();
        self.run_exists(run_id)?;
        self.database
            .team_transaction(|connection| remove_run(connection, run_id))?;
        Ok(())
    }

    /// Adds a member to a run. An agent may start under an orchestrator; without one it gets no task.
    pub fn add_member(&self, run_id: &str, input: NewTeamMember) -> Result<TeamMember, TeamError> {
        let _turn = self.turn();
        self.run_exists(run_id)?;
        let members = self.database.team_members()?;
        let member = match input {
            NewTeamMember::Agent {
                role,
                harness,
                parent_id,
                orchestrator,
                task,
                acceptance,
            } => {
                let mut member = TeamMember {
                    harness: Some(harness),
                    orchestrator,
                    ..blank_member(run_id, TeamMemberKind::Agent, clean_role(&role)?)
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
            NewTeamMember::Bot { bot_id, role } => {
                self.check_bot(&bot_id)?;
                check_bot_join(&members, run_id, &bot_id)?;
                TeamMember {
                    bot_id: Some(bot_id),
                    orchestrator: true,
                    ..blank_member(run_id, TeamMemberKind::Bot, clean_role(&role)?)
                }
            }
        };
        self.database
            .team_transaction(|connection| insert_member(connection, &member))?;
        Ok(member)
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

    /// Removes a member. Its lines go with it. The members it orchestrated answer to no one, and lose their task.
    pub fn dismiss_member(&self, member_id: &str) -> Result<(), TeamError> {
        let _turn = self.turn();
        member_of(&self.database.team_members()?, member_id)?;
        self.database.team_transaction(|connection| {
            clear_tasks_of(connection, member_id)?;
            remove_member(connection, member_id)
        })?;
        Ok(())
    }

    /// Removes a bot from every run: its members, the members under them at any depth, and their lines. Nothing
    /// refuses it, since the person already deleted the bot. Returns how many members went.
    pub fn remove_bot(&self, bot_id: &str) -> Result<usize, TeamError> {
        let _turn = self.turn();
        let gone = bot_and_its_members(&self.database.team_members()?, bot_id);
        self.database.team_transaction(|connection| {
            for id in &gone {
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
fn blank_member(run_id: &str, kind: TeamMemberKind, role: String) -> TeamMember {
    TeamMember {
        id: new_id(),
        run_id: run_id.to_owned(),
        kind,
        role,
        harness: None,
        bot_id: None,
        parent_id: None,
        orchestrator: false,
        task: String::new(),
        acceptance: String::new(),
    }
}

/// The bot's members in every run, then every member under them at any depth. Each id is listed once, so a
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
