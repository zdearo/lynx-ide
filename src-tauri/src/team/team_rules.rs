//! The team's rules, checked against the members and lines a run holds. Nothing here writes: the
//! service applies a change only once these say it is allowed.

use crate::bots::bot_folder::valid_id;

use super::team_error::TeamError;
use super::team_model::{TeamMember, TeamMemberKind, TeamPair};

pub const MAX_TITLE_CHARS: usize = 80;
pub const MAX_ROLE_CHARS: usize = 40;
pub const MAX_TEXT_CHARS: usize = 2000;

/// The run's goal, trimmed. It must not be empty.
pub fn clean_title(title: &str) -> Result<String, TeamError> {
    let title = title.trim();
    if title.is_empty() || title.chars().count() > MAX_TITLE_CHARS {
        return Err(TeamError::InvalidTitle);
    }
    Ok(title.to_owned())
}

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
/// orchestrator of the same run that is not under the child.
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
    if parent.run_id != child.run_id {
        return Err(TeamError::OtherRun);
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

/// Whether two members may talk: different and in the same run, not already talking, not parent and child,
/// and not two bots.
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
    if first.run_id != second.run_id {
        return Err(TeamError::OtherRun);
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

/// A bot joins a run once, so its id is not in the run already.
pub fn check_bot_join(members: &[TeamMember], run_id: &str, bot_id: &str) -> Result<(), TeamError> {
    let taken = members.iter().any(|member| {
        member.run_id == run_id
            && member.kind == TeamMemberKind::Bot
            && member.bot_id.as_deref() == Some(bot_id)
    });
    if taken {
        return Err(TeamError::BotInRun);
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

#[cfg(test)]
mod tests {
    use crate::agents::harness::HarnessId;

    use super::*;

    fn agent(id: &str, parent: Option<&str>, orchestrator: bool) -> TeamMember {
        TeamMember {
            id: id.to_owned(),
            run_id: "run".to_owned(),
            kind: TeamMemberKind::Agent,
            role: id.to_owned(),
            harness: Some(HarnessId::Claude),
            bot_id: None,
            parent_id: parent.map(str::to_owned),
            orchestrator,
            task: String::new(),
            acceptance: String::new(),
        }
    }

    #[test]
    fn a_title_and_a_role_are_trimmed_and_bounded() {
        assert_eq!(
            clean_title("  Auth refactor  ").ok().as_deref(),
            Some("Auth refactor")
        );
        assert!(matches!(clean_title("   "), Err(TeamError::InvalidTitle)));
        assert!(clean_title(&"x".repeat(MAX_TITLE_CHARS)).is_ok());
        assert!(matches!(
            clean_title(&"x".repeat(MAX_TITLE_CHARS + 1)),
            Err(TeamError::InvalidTitle)
        ));
        assert!(matches!(clean_role(""), Err(TeamError::InvalidRole)));
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
        // lead is the root, builder answers to lead, tester answers to builder and is not an orchestrator.
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
