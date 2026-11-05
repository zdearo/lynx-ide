//! The team's rules, checked against the members and lines of a worktree's team. Nothing here writes: the
//! service applies a change only once these say it is allowed.

use std::path::{Component, Path};

use crate::bots::bot_folder::valid_id;

use super::team_error::TeamError;
use super::team_model::{TaskState, TeamMember, TeamMemberKind, TeamPair};

pub const MAX_ROLE_CHARS: usize = 40;
pub const MAX_TEXT_CHARS: usize = 2000;
pub const MAX_SESSION_ID_CHARS: usize = 128;

/// A role, trimmed. It must not be empty.
pub fn clean_role(role: &str) -> Result<String, TeamError> {
    let role = role.trim();
    if role.is_empty() || role.chars().count() > MAX_ROLE_CHARS {
        return Err(TeamError::InvalidRole);
    }
    Ok(role.to_owned())
}

/// A task or a "done when" line, trimmed. It may be empty: the user can clear it.
pub fn clean_text(text: &str) -> Result<String, TeamError> {
    let text = text.trim();
    if text.chars().count() > MAX_TEXT_CHARS {
        return Err(TeamError::InvalidText);
    }
    Ok(text.to_owned())
}

/// A bot id is a folder name under the bots folder, so it must be one.
pub fn check_bot_id(bot_id: &str) -> Result<(), TeamError> {
    if valid_id(bot_id) {
        Ok(())
    } else {
        Err(TeamError::InvalidBot)
    }
}

/// A worktree a member joins: an absolute path with no `..` step, so it names one place.
pub fn clean_worktree(worktree: &str) -> Result<String, TeamError> {
    let path = Path::new(worktree);
    let plain = !worktree.is_empty()
        && path.is_absolute()
        && path.components().all(|part| part != Component::ParentDir);
    if plain {
        Ok(worktree.to_owned())
    } else {
        Err(TeamError::InvalidWorktree)
    }
}

/// A terminal session id, as the terminal makes it: letters, digits, `-` and `_`, the same plain
/// names a resumed conversation takes.
pub fn clean_session_id(session_id: &str) -> Result<String, TeamError> {
    let plain = !session_id.is_empty()
        && session_id.chars().count() <= MAX_SESSION_ID_CHARS
        && session_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if plain {
        Ok(session_id.to_owned())
    } else {
        Err(TeamError::InvalidSession)
    }
}

/// A bot runs no terminal session, so only an agent takes one.
pub fn check_agent_only(member: &TeamMember) -> Result<(), TeamError> {
    if member.kind == TeamMemberKind::Bot {
        return Err(TeamError::BotSession);
    }
    Ok(())
}

pub fn member_of<'a>(members: &'a [TeamMember], id: &str) -> Result<&'a TeamMember, TeamError> {
    members
        .iter()
        .find(|member| member.id == id)
        .ok_or_else(|| TeamError::MemberNotFound(id.to_owned()))
}

/// Whether `member` answers, directly or through others, to the member `ancestor_id`. The walk stops
/// after as many steps as there are members, so a corrupt loop that does not include the ancestor ends.
fn is_under(members: &[TeamMember], member: &TeamMember, ancestor_id: &str) -> bool {
    let mut current = member.parent_id.as_deref();
    for _ in 0..members.len() {
        let Some(id) = current else {
            return false;
        };
        if id == ancestor_id {
            return true;
        }
        current = member_of(members, id)
            .ok()
            .and_then(|above| above.parent_id.as_deref());
    }
    false
}

/// Whether `child` may answer to `parent_id`: a bot never does, and the parent must be another
/// orchestrator of the same worktree that is not under the child.
pub fn check_parent(
    members: &[TeamMember],
    child: &TeamMember,
    parent_id: &str,
) -> Result<(), TeamError> {
    if child.kind == TeamMemberKind::Bot {
        return Err(TeamError::BotHasParent);
    }
    let parent = member_of(members, parent_id)?;
    if parent.id == child.id {
        return Err(TeamError::SameMember);
    }
    if parent.worktree != child.worktree {
        return Err(TeamError::OtherWorktree);
    }
    if !parent.orchestrator {
        return Err(TeamError::NotAnOrchestrator);
    }
    if is_under(members, parent, &child.id) {
        return Err(TeamError::Cycle);
    }
    Ok(())
}

/// Whether a member may stop being an orchestrator: a bot may not, and a member with members under it may not.
pub fn check_orchestrator_off(members: &[TeamMember], id: &str) -> Result<(), TeamError> {
    let member = member_of(members, id)?;
    if member.kind == TeamMemberKind::Bot {
        return Err(TeamError::BotNotOrchestrator);
    }
    if members
        .iter()
        .any(|other| other.parent_id.as_deref() == Some(id))
    {
        return Err(TeamError::HasMembers);
    }
    Ok(())
}

/// Whether two members may talk: different and in the same worktree, not already talking, not parent and
/// child, and not two bots.
pub fn check_pair(
    members: &[TeamMember],
    pairs: &[TeamPair],
    a: &str,
    b: &str,
) -> Result<(), TeamError> {
    let first = member_of(members, a)?;
    let second = member_of(members, b)?;
    if a == b {
        return Err(TeamError::SameMember);
    }
    if first.worktree != second.worktree {
        return Err(TeamError::OtherWorktree);
    }
    let pair = TeamPair::between(a, b);
    if pairs.contains(&pair) {
        return Err(TeamError::AlreadyLinked);
    }
    if first.parent_id.as_deref() == Some(b) || second.parent_id.as_deref() == Some(a) {
        return Err(TeamError::LinkedToParent);
    }
    if first.kind == TeamMemberKind::Bot && second.kind == TeamMemberKind::Bot {
        return Err(TeamError::BotPair);
    }
    Ok(())
}

/// A bot joins a worktree's team once, so its id is not in that team already. It may join another worktree's.
pub fn check_bot_join(
    members: &[TeamMember],
    worktree: &str,
    bot_id: &str,
) -> Result<(), TeamError> {
    let taken = members.iter().any(|member| {
        member.worktree == worktree
            && member.kind == TeamMemberKind::Bot
            && member.bot_id.as_deref() == Some(bot_id)
    });
    if taken {
        return Err(TeamError::BotInWorktree);
    }
    Ok(())
}

/// A terminal session belongs to one member at most. `member_id` is the member that may already hold it.
pub fn check_session_free(
    members: &[TeamMember],
    session_id: &str,
    member_id: Option<&str>,
) -> Result<(), TeamError> {
    let held = members.iter().any(|member| {
        member.session_id.as_deref() == Some(session_id) && Some(member.id.as_str()) != member_id
    });
    if held {
        return Err(TeamError::SessionTaken);
    }
    Ok(())
}

/// Only an agent takes a new role; a bot's role is its name.
pub fn check_role_change(member: &TeamMember) -> Result<(), TeamError> {
    if member.kind == TeamMemberKind::Bot {
        return Err(TeamError::BotRole);
    }
    Ok(())
}

/// Only a member with an orchestrator above it has a task.
pub fn check_has_orchestrator(member: &TeamMember) -> Result<(), TeamError> {
    if member.parent_id.is_none() {
        return Err(TeamError::NoOrchestrator);
    }
    Ok(())
}

/// Only a member whose task has not started yet may change what it waits for.
pub fn check_task_pending(member: &TeamMember) -> Result<(), TeamError> {
    if member.task_state != Some(TaskState::Pending) {
        return Err(TeamError::TaskStarted);
    }
    Ok(())
}

/// What `member` may wait for: members of its own worktree, never itself, and never one that waits for it through
/// any chain. `edges` are the stored lines (member, waits for), without the member's own lines, which are replaced.
pub fn check_dependencies(
    members: &[TeamMember],
    edges: &[(String, String)],
    member: &TeamMember,
    depends_on: &[String],
) -> Result<(), TeamError> {
    for id in depends_on {
        let dependency = member_of(members, id)?;
        if dependency.id == member.id {
            return Err(TeamError::DependencyCycle);
        }
        if dependency.worktree != member.worktree {
            return Err(TeamError::DependencyOtherWorktree);
        }
        if waits_for(edges, &dependency.id, &member.id) {
            return Err(TeamError::DependencyCycle);
        }
    }
    Ok(())
}

/// Whether `from` waits for `target`, directly or through others. Each member is visited once, so a loop ends.
fn waits_for(edges: &[(String, String)], from: &str, target: &str) -> bool {
    let mut stack = vec![from];
    let mut seen: Vec<&str> = Vec::new();
    while let Some(current) = stack.pop() {
        if current == target {
            return true;
        }
        if seen.contains(&current) {
            continue;
        }
        seen.push(current);
        stack.extend(
            edges
                .iter()
                .filter(|(member, _)| member == current)
                .map(|(_, dependency)| dependency.as_str()),
        );
    }
    false
}

#[cfg(test)]
mod tests {
    use crate::agents::harness::HarnessId;

    use super::*;

    fn agent(id: &str, parent: Option<&str>, orchestrator: bool) -> TeamMember {
        TeamMember {
            id: id.to_owned(),
            worktree: "/repo".to_owned(),
            kind: TeamMemberKind::Agent,
            role: id.to_owned(),
            harness: Some(HarnessId::Claude),
            bot_id: None,
            parent_id: parent.map(str::to_owned),
            orchestrator,
            task: String::new(),
            acceptance: String::new(),
            session_id: None,
            expanded: false,
            worker_state: None,
            task_state: None,
            depends_on: Vec::new(),
            dispatch_state: None,
            execution_worktree: None,
        }
    }

    #[test]
    fn a_role_is_trimmed_and_bounded() {
        assert_eq!(clean_role("  Tester  ").ok().as_deref(), Some("Tester"));
        assert!(matches!(clean_role("   "), Err(TeamError::InvalidRole)));
        assert!(matches!(
            clean_role(&"x".repeat(MAX_ROLE_CHARS + 1)),
            Err(TeamError::InvalidRole)
        ));
    }

    #[test]
    fn a_task_may_be_cleared_but_not_made_too_long() {
        assert_eq!(clean_text("  ").ok().as_deref(), Some(""));
        assert!(matches!(
            clean_text(&"x".repeat(MAX_TEXT_CHARS + 1)),
            Err(TeamError::InvalidText)
        ));
    }

    #[test]
    fn a_parent_cycle_is_refused_and_the_other_parent_rules_still_apply() {
        // lead is a root, builder answers to lead, tester answers to builder and is not an orchestrator.
        let members = vec![
            agent("lead", None, true),
            agent("builder", Some("lead"), true),
            agent("tester", Some("builder"), false),
        ];
        // The builder is not under the tester, so the tester may answer to it.
        assert!(check_parent(&members, &members[2], "builder").is_ok());
        // The lead cannot answer to its own member: the builder answers to the lead.
        assert!(matches!(
            check_parent(&members, &members[0], "builder"),
            Err(TeamError::Cycle)
        ));
        // A member that is not an orchestrator takes no members.
        assert!(matches!(
            check_parent(&members, &members[0], "tester"),
            Err(TeamError::NotAnOrchestrator)
        ));
        assert!(matches!(
            check_parent(&members, &members[1], "builder"),
            Err(TeamError::SameMember)
        ));
    }

    #[test]
    fn a_loop_that_never_reaches_the_ancestor_ends() {
        let looped = vec![agent("a", Some("b"), true), agent("b", Some("a"), true)];
        assert!(!is_under(&looped, &looped[0], "missing"));
    }
}
