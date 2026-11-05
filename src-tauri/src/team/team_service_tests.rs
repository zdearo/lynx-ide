//! The team's changes against a real database in a temporary folder: each rule, its refusal, and what a
//! change leaves behind. A team is the members of one worktree.

use std::sync::Arc;

use crate::agents::harness::HarnessId;
use crate::persistence::AppDatabase;

use super::team_error::TeamError;
use super::team_model::{NewTeamMember, TeamMember, TeamMemberKind, TeamPair};
use super::team_service::TeamService;

fn service() -> (TeamService, tempfile::TempDir) {
    service_with_bots(|_| true)
}

fn service_with_bots(bot_exists: fn(&str) -> bool) -> (TeamService, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let database = AppDatabase::open(&dir.path().join("app.db")).expect("open");
    (
        TeamService::new(Arc::new(database), Box::new(bot_exists)),
        dir,
    )
}

const WORKTREE: &str = "/home/dev/repo-worktrees/auth";
const OTHER_WORKTREE: &str = "/home/dev/repo-worktrees/docs";
const SESSION: &str = "3f2a9c1e-7b4d-4e8a-9c0f-1a2b3c4d5e6f";
const RESTORED: &str = "9d8c7b6a-5f4e-4d3c-8b2a-1f0e9d8c7b6a";

fn agent_in(worktree: &str, role: &str, parent: Option<&str>, orchestrator: bool) -> NewTeamMember {
    NewTeamMember::Agent {
        role: role.to_owned(),
        harness: Some(HarnessId::Codex),
        parent_id: parent.map(str::to_owned),
        orchestrator,
        task: "write the form".to_owned(),
        acceptance: "the form submits".to_owned(),
        depends_on: Vec::new(),
        worktree: worktree.to_owned(),
        session_id: None,
    }
}

fn agent(role: &str, parent: Option<&str>, orchestrator: bool) -> NewTeamMember {
    agent_in(WORKTREE, role, parent, orchestrator)
}

fn placed_agent(role: &str, worktree: &str, session_id: Option<&str>) -> NewTeamMember {
    NewTeamMember::Agent {
        role: role.to_owned(),
        harness: Some(HarnessId::Claude),
        parent_id: None,
        orchestrator: false,
        task: String::new(),
        acceptance: String::new(),
        depends_on: Vec::new(),
        worktree: worktree.to_owned(),
        session_id: session_id.map(str::to_owned),
    }
}

fn bot_in(worktree: &str, bot_id: &str, role: &str) -> NewTeamMember {
    NewTeamMember::Bot {
        bot_id: bot_id.to_owned(),
        role: role.to_owned(),
        worktree: worktree.to_owned(),
    }
}

fn bot(bot_id: &str, role: &str) -> NewTeamMember {
    bot_in(WORKTREE, bot_id, role)
}

fn members_of(service: &TeamService, worktree: &str) -> Vec<TeamMember> {
    service
        .snapshot()
        .expect("snapshot")
        .members
        .into_iter()
        .filter(|member| member.worktree == worktree)
        .collect()
}

fn get(service: &TeamService, id: &str) -> TeamMember {
    service
        .snapshot()
        .expect("snapshot")
        .members
        .into_iter()
        .find(|member| member.id == id)
        .expect("member exists")
}

fn add(service: &TeamService, member: NewTeamMember) -> TeamMember {
    service.add_member(member).expect("add")
}

#[test]
fn a_new_team_is_empty_and_its_first_member_is_the_root_of_a_tree() {
    let (service, _dir) = service();
    assert!(service.snapshot().expect("snapshot").members.is_empty());

    let root = add(&service, agent("Builder", None, true));

    assert_eq!(root.role, "Builder");
    assert_eq!(root.worktree, WORKTREE);
    assert_eq!(root.kind, TeamMemberKind::Agent);
    assert_eq!(root.harness, Some(HarnessId::Codex));
    assert!(root.orchestrator);
    assert_eq!(root.parent_id, None);
    assert_eq!(root.task, "", "a root has no task");
    assert_eq!(
        service.snapshot().expect("snapshot").members.len(),
        1,
        "no lead is made for it"
    );
}

#[test]
fn a_bot_joins_a_worktree_as_an_orchestrator_and_a_bad_id_is_refused() {
    let (service, _dir) = service();

    let docs = add(&service, bot("docs", "Docs"));

    assert_eq!(docs.kind, TeamMemberKind::Bot);
    assert_eq!(docs.bot_id.as_deref(), Some("docs"));
    assert_eq!(docs.harness, None);
    assert!(docs.orchestrator);
    assert_eq!(docs.worktree, WORKTREE);
    assert_eq!(docs.session_id, None);
    assert!(matches!(
        service.add_member(bot("Not A Bot", "Other")),
        Err(TeamError::InvalidBot)
    ));
}

#[test]
fn add_member_refuses_a_parent_that_is_not_an_orchestrator() {
    let (service, _dir) = service();
    let lead = add(&service, agent("Lead", None, true));
    let worker = add(&service, agent("Worker", Some(&lead.id), false));

    assert!(matches!(
        service.add_member(agent("Tester", Some(&worker.id), false)),
        Err(TeamError::NotAnOrchestrator)
    ));
}

#[test]
fn add_member_keeps_its_task_only_under_an_orchestrator() {
    let (service, _dir) = service();
    let lead = add(&service, agent("Lead", None, true));

    let builder = add(&service, agent("Builder", Some(&lead.id), true));
    let solo = add(&service, agent("Reviewer", None, false));

    assert_eq!(builder.parent_id.as_deref(), Some(lead.id.as_str()));
    assert_eq!(builder.task, "write the form");
    assert_eq!(solo.parent_id, None);
    assert_eq!(solo.task, "");
    assert_eq!(solo.acceptance, "");
}

#[test]
fn add_member_refuses_an_empty_or_long_role_and_trims_the_role() {
    let (service, _dir) = service();

    assert!(matches!(
        service.add_member(agent("   ", None, false)),
        Err(TeamError::InvalidRole)
    ));
    assert!(matches!(
        service.add_member(agent(&"x".repeat(41), None, false)),
        Err(TeamError::InvalidRole)
    ));
    let tester = add(&service, agent("  Tester  ", None, false));
    assert_eq!(tester.role, "Tester");
}

#[test]
fn add_member_refuses_a_parent_from_another_worktree_and_one_that_is_not_there() {
    let (service, _dir) = service();
    let docs_lead = add(&service, agent_in(OTHER_WORKTREE, "Lead", None, true));
    add(&service, agent("Lead", None, true));

    assert!(matches!(
        service.add_member(agent("Worker", Some(&docs_lead.id), false)),
        Err(TeamError::OtherWorktree)
    ));
    assert!(matches!(
        service.add_member(agent("Worker", Some("missing"), false)),
        Err(TeamError::MemberNotFound(_))
    ));
    assert_eq!(members_of(&service, WORKTREE).len(), 1, "only the lead");
}

#[test]
fn set_parent_refuses_a_cycle_and_a_member_under_itself_and_changes_nothing() {
    let (service, _dir) = service();
    let lead = add(&service, agent("Lead", None, true));
    let builder = add(&service, agent("Builder", Some(&lead.id), true));

    assert!(matches!(
        service.set_parent(&lead.id, Some(&builder.id)),
        Err(TeamError::Cycle)
    ));
    assert!(matches!(
        service.set_parent(&builder.id, Some(&builder.id)),
        Err(TeamError::SameMember)
    ));
    assert_eq!(get(&service, &lead.id).parent_id, None);
    assert_eq!(
        get(&service, &builder.id).parent_id.as_deref(),
        Some(lead.id.as_str())
    );
}

#[test]
fn set_parent_refuses_a_bot_as_a_child() {
    let (service, _dir) = service();
    let docs = add(&service, bot("docs", "Docs"));
    let helper = add(&service, agent("Helper", None, true));

    assert!(matches!(
        service.set_parent(&docs.id, Some(&helper.id)),
        Err(TeamError::BotHasParent)
    ));
}

#[test]
fn set_parent_none_detaches_a_member_and_clears_its_task() {
    let (service, _dir) = service();
    let lead = add(&service, agent("Lead", None, true));
    let builder = add(&service, agent("Builder", Some(&lead.id), true));

    service.set_parent(&builder.id, None).expect("detach");

    let detached = get(&service, &builder.id);
    assert_eq!(detached.parent_id, None);
    assert_eq!(detached.task, "");
    assert_eq!(detached.acceptance, "");
    // Attaching it again does not bring the old task back.
    service
        .set_parent(&builder.id, Some(&lead.id))
        .expect("attach");
    assert_eq!(get(&service, &builder.id).task, "");
}

#[test]
fn set_parent_drops_the_line_between_the_parent_and_the_child() {
    let (service, _dir) = service();
    let lead = add(&service, agent("Lead", None, true));
    let reviewer = add(&service, agent("Reviewer", None, false));
    service
        .set_linked(&lead.id, &reviewer.id, true)
        .expect("link");
    assert_eq!(service.snapshot().expect("snapshot").pairs.len(), 1);

    service
        .set_parent(&reviewer.id, Some(&lead.id))
        .expect("attach");

    assert!(service.snapshot().expect("snapshot").pairs.is_empty());
}

#[test]
fn an_orchestrator_with_members_under_it_cannot_stop_being_one() {
    let (service, _dir) = service();
    let lead = add(&service, agent("Lead", None, true));
    let builder = add(&service, agent("Builder", Some(&lead.id), false));

    assert!(matches!(
        service.set_orchestrator(&lead.id, false),
        Err(TeamError::HasMembers)
    ));
    service.set_parent(&builder.id, None).expect("detach");
    service
        .set_orchestrator(&lead.id, false)
        .expect("no members left under it");
    assert!(!get(&service, &lead.id).orchestrator);
}

#[test]
fn a_bot_stays_an_orchestrator() {
    let (service, _dir) = service();
    let docs = add(&service, bot("docs", "Docs"));

    assert!(matches!(
        service.set_orchestrator(&docs.id, false),
        Err(TeamError::BotNotOrchestrator)
    ));
    assert!(get(&service, &docs.id).orchestrator);
}

#[test]
fn a_bot_joins_each_worktree_once_and_may_join_two_worktrees() {
    let (service, _dir) = service();
    add(&service, bot("docs", "Docs"));

    assert!(matches!(
        service.add_member(bot("docs", "Docs")),
        Err(TeamError::BotInWorktree)
    ));
    add(&service, bot_in(OTHER_WORKTREE, "docs", "Docs"));

    assert_eq!(members_of(&service, WORKTREE).len(), 1);
    assert_eq!(members_of(&service, OTHER_WORKTREE).len(), 1);
}

#[test]
fn a_bot_that_does_not_exist_cannot_join_a_team() {
    let (service, _dir) = service_with_bots(|id| id == "docs");
    add(&service, agent("Lead", None, true));

    assert!(matches!(
        service.add_member(bot("ghost", "Ghost")),
        Err(TeamError::UnknownBot)
    ));
    assert!(matches!(
        service.add_member(bot_in(OTHER_WORKTREE, "ghost", "Ghost")),
        Err(TeamError::UnknownBot)
    ));
    assert_eq!(members_of(&service, WORKTREE).len(), 1, "only the lead");
}

#[test]
fn an_existing_bot_joins_a_team() {
    let (service, _dir) = service_with_bots(|id| id == "docs");

    let member = add(&service, bot("docs", "Docs"));

    assert_eq!(member.bot_id.as_deref(), Some("docs"));
    assert_eq!(members_of(&service, WORKTREE).len(), 1);
}

#[test]
fn set_role_changes_an_agent_and_refuses_a_bot() {
    let (service, _dir) = service();
    let docs = add(&service, bot("docs", "Docs"));
    let writer = add(&service, agent("Writer", None, false));

    service.set_role(&writer.id, "  Editor  ").expect("role");
    assert_eq!(get(&service, &writer.id).role, "Editor");
    assert!(matches!(
        service.set_role(&writer.id, ""),
        Err(TeamError::InvalidRole)
    ));
    assert!(matches!(
        service.set_role(&docs.id, "Boss"),
        Err(TeamError::BotRole)
    ));
}

#[test]
fn set_task_needs_an_orchestrator_above_the_member() {
    let (service, _dir) = service();
    let lead = add(&service, agent("Lead", None, true));
    let solo = add(&service, agent("Solo", None, false));
    let builder = add(&service, agent("Builder", Some(&lead.id), true));

    assert!(matches!(
        service.set_task(&solo.id, "login", "tests pass"),
        Err(TeamError::NoOrchestrator)
    ));
    service
        .set_task(&builder.id, "  login  ", "tests pass")
        .expect("task");
    let set = get(&service, &builder.id);
    assert_eq!(set.task, "login");
    assert_eq!(set.acceptance, "tests pass");
    assert!(matches!(
        service.set_task(&builder.id, &"x".repeat(2001), "tests pass"),
        Err(TeamError::InvalidText)
    ));
}

#[test]
fn set_linked_refuses_the_invalid_lines() {
    let (service, _dir) = service();
    let lead = add(&service, agent("Lead", None, true));
    let builder = add(&service, agent("Builder", Some(&lead.id), true));
    let reviewer = add(&service, agent("Reviewer", None, false));
    let writer = add(&service, agent_in(OTHER_WORKTREE, "Writer", None, false));

    assert!(matches!(
        service.set_linked(&reviewer.id, &reviewer.id, true),
        Err(TeamError::SameMember)
    ));
    assert!(matches!(
        service.set_linked(&lead.id, &builder.id, true),
        Err(TeamError::LinkedToParent)
    ));
    assert!(matches!(
        service.set_linked(&reviewer.id, &writer.id, true),
        Err(TeamError::OtherWorktree)
    ));
    service
        .set_linked(&builder.id, &reviewer.id, true)
        .expect("link");
    assert!(matches!(
        service.set_linked(&reviewer.id, &builder.id, true),
        Err(TeamError::AlreadyLinked)
    ));
}

#[test]
fn two_bots_do_not_talk_and_a_bot_talks_with_an_agent() {
    let (service, _dir) = service();
    let docs = add(&service, bot("docs", "Docs"));
    let second_bot = add(&service, bot("writer-bot", "Writer bot"));
    let writer = add(&service, agent("Writer", None, false));

    assert!(matches!(
        service.set_linked(&docs.id, &second_bot.id, true),
        Err(TeamError::BotPair)
    ));
    service
        .set_linked(&docs.id, &writer.id, true)
        .expect("a bot takes messages from an agent");
}

#[test]
fn a_line_is_stored_once_with_the_lower_id_first_and_removing_it_is_always_allowed() {
    let (service, _dir) = service();
    let lead = add(&service, agent("Lead", None, true));
    let reviewer = add(&service, agent("Reviewer", None, false));

    service
        .set_linked(&reviewer.id, &lead.id, true)
        .expect("link");
    let pairs = service.snapshot().expect("snapshot").pairs;
    assert_eq!(pairs.len(), 1);
    assert!(pairs[0].a < pairs[0].b);

    service
        .set_linked(&lead.id, &reviewer.id, false)
        .expect("unlink");
    service
        .set_linked(&lead.id, &reviewer.id, false)
        .expect("unlinking a line that is not there is fine");
    assert!(service.snapshot().expect("snapshot").pairs.is_empty());
}

#[test]
fn dismissing_an_orchestrator_leaves_its_members_without_a_parent_or_task() {
    let (service, _dir) = service();
    let lead = add(&service, agent("Lead", None, true));
    let builder = add(&service, agent("Builder", Some(&lead.id), true));
    let tester = add(&service, agent("Tester", Some(&builder.id), false));
    let reviewer = add(&service, agent("Reviewer", None, false));
    service
        .set_linked(&builder.id, &reviewer.id, true)
        .expect("link");

    service.dismiss_member(&builder.id).expect("dismiss");

    let orphan = get(&service, &tester.id);
    assert_eq!(orphan.parent_id, None);
    assert_eq!(orphan.task, "");
    assert_eq!(orphan.acceptance, "");
    let snapshot = service.snapshot().expect("snapshot");
    assert!(
        snapshot.pairs.is_empty(),
        "the dismissed member's line goes"
    );
    assert_eq!(members_of(&service, WORKTREE).len(), 3);
}

#[test]
fn dismissing_a_member_that_is_not_there_is_refused() {
    let (service, _dir) = service();

    assert!(matches!(
        service.dismiss_member("missing"),
        Err(TeamError::MemberNotFound(_))
    ));
}

#[test]
fn a_change_to_a_member_or_a_parent_that_is_not_there_is_refused() {
    let (service, _dir) = service();
    let lead = add(&service, agent("Lead", None, true));

    assert!(matches!(
        service.set_role("missing", "Editor"),
        Err(TeamError::MemberNotFound(_))
    ));
    assert!(matches!(
        service.set_parent(&lead.id, Some("missing")),
        Err(TeamError::MemberNotFound(_))
    ));
    assert!(matches!(
        service.set_linked(&lead.id, "missing", true),
        Err(TeamError::MemberNotFound(_))
    ));
}

#[test]
fn removing_a_bot_takes_the_members_under_it_and_their_lines_in_every_worktree() {
    let (service, _dir) = service();
    let docs = add(&service, bot("docs", "Docs"));
    let builder = add(&service, agent("Builder", Some(&docs.id), true));
    add(&service, agent("Tester", Some(&builder.id), false));
    let reviewer = add(&service, agent("Reviewer", None, false));
    let writer_bot = add(&service, bot("writer-bot", "Writer bot"));
    service
        .set_linked(&builder.id, &reviewer.id, true)
        .expect("link");
    service
        .set_linked(&docs.id, &reviewer.id, true)
        .expect("link");
    let other_docs = add(&service, bot_in(OTHER_WORKTREE, "docs", "Docs"));
    add(
        &service,
        agent_in(OTHER_WORKTREE, "Writer", Some(&other_docs.id), false),
    );

    let removed = service.remove_bot("docs").expect("remove");

    // The first worktree loses the bot, its builder and the tester under the builder; the other one loses
    // its bot and the writer under it.
    assert_eq!(removed, 5);
    let snapshot = service.snapshot().expect("snapshot");
    let left: Vec<String> = snapshot
        .members
        .iter()
        .map(|member| member.id.clone())
        .collect();
    assert_eq!(left, [reviewer.id.clone(), writer_bot.id.clone()]);
    assert!(
        snapshot.pairs.is_empty(),
        "every line to a removed member goes"
    );
    assert_eq!(service.remove_bot("docs").expect("again"), 0);
}

#[test]
fn removing_a_bot_leaves_the_agents_that_were_not_under_it_as_they_were() {
    let (service, _dir) = service();
    let lead = add(&service, agent("Lead", None, true));
    let builder = add(&service, agent("Builder", Some(&lead.id), true));
    let worker = add(&service, agent("Worker", Some(&builder.id), false));
    let docs = add(&service, bot("docs", "Docs"));
    service
        .set_linked(&docs.id, &worker.id, true)
        .expect("link");
    let before = (get(&service, &builder.id), get(&service, &worker.id));

    assert_eq!(service.remove_bot("docs").expect("remove"), 1);

    assert_eq!(
        (get(&service, &builder.id), get(&service, &worker.id)),
        before
    );
    assert!(service.snapshot().expect("snapshot").pairs.is_empty());
}

#[test]
fn removing_a_bot_that_is_not_in_the_team_changes_nothing() {
    let (service, _dir) = service();
    add(&service, bot("docs", "Docs"));
    let before = service.snapshot().expect("snapshot");

    assert_eq!(service.remove_bot("ninguem").expect("remove"), 0);

    assert_eq!(service.snapshot().expect("snapshot"), before);
}

#[test]
fn an_agent_keeps_its_worktree_and_session_and_they_come_back_in_the_snapshot() {
    let (service, _dir) = service();
    let builder = add(&service, placed_agent("Builder", WORKTREE, Some(SESSION)));

    assert_eq!(builder.worktree, WORKTREE);
    assert_eq!(builder.session_id.as_deref(), Some(SESSION));
    let stored = get(&service, &builder.id);
    assert_eq!(stored.worktree, WORKTREE);
    assert_eq!(stored.session_id.as_deref(), Some(SESSION));
    let json = serde_json::to_value(&stored).expect("json");
    assert_eq!(json["worktree"], WORKTREE);
    assert_eq!(json["sessionId"], SESSION);
    assert!(json.get("runId").is_none(), "a member names no run");
    let plain = add(&service, placed_agent("Reviewer", WORKTREE, None));
    assert_eq!(plain.worktree, WORKTREE);
    assert_eq!(plain.session_id, None);
}

#[test]
fn a_bot_never_gets_a_session_and_its_worktree_is_checked_like_any_member() {
    let (service, _dir) = service();
    let docs = add(&service, bot("docs", "Docs"));

    assert_eq!(docs.worktree, WORKTREE);
    assert!(matches!(
        service.set_session(&docs.id, Some(SESSION)),
        Err(TeamError::BotSession)
    ));
    assert!(matches!(
        service.set_session(&docs.id, None),
        Err(TeamError::BotSession)
    ));
    assert_eq!(get(&service, &docs.id).session_id, None);
    assert!(matches!(
        service.add_member(bot_in("relative/worktree", "writer-bot", "Writer bot")),
        Err(TeamError::InvalidWorktree)
    ));
    assert_eq!(members_of(&service, WORKTREE).len(), 1);
}

#[test]
fn an_agent_may_run_with_or_without_a_session_in_its_worktree() {
    let (service, _dir) = service();
    let plain = add(&service, placed_agent("Tester", WORKTREE, None));
    assert_eq!(plain.session_id, None);

    service
        .set_session(&plain.id, Some(SESSION))
        .expect("a session in its worktree");

    let attached = get(&service, &plain.id);
    assert_eq!(attached.session_id.as_deref(), Some(SESSION));
    assert_eq!(attached.worktree, WORKTREE);
}

#[test]
fn a_worktree_is_an_absolute_path_with_no_parent_steps() {
    let (service, _dir) = service();

    for worktree in ["", "relative/worktree", "/repo/../elsewhere"] {
        assert!(
            matches!(
                service.add_member(placed_agent("Builder", worktree, Some(SESSION))),
                Err(TeamError::InvalidWorktree)
            ),
            "{worktree:?} must be refused"
        );
        assert!(
            matches!(
                service.add_member(bot_in(worktree, "docs", "Docs")),
                Err(TeamError::InvalidWorktree)
            ),
            "{worktree:?} must be refused for a bot"
        );
    }
    add(&service, placed_agent("Builder", WORKTREE, Some(SESSION)));
}

#[test]
fn a_session_id_is_a_plain_name_of_at_most_128_characters() {
    let (service, _dir) = service();
    let too_long = "x".repeat(129);

    for session in ["", "has space", "semi;colon", too_long.as_str()] {
        assert!(
            matches!(
                service.add_member(placed_agent("Builder", WORKTREE, Some(session))),
                Err(TeamError::InvalidSession)
            ),
            "{session:?} must be refused"
        );
    }
    add(
        &service,
        placed_agent("Builder", WORKTREE, Some(&"x".repeat(128))),
    );
}

#[test]
fn dismissing_an_agent_returns_the_session_it_ran_in() {
    let (service, _dir) = service();
    let builder = add(&service, placed_agent("Builder", WORKTREE, Some(SESSION)));
    let plain = add(&service, placed_agent("Reviewer", WORKTREE, None));

    assert_eq!(
        service.dismiss_member(&builder.id).expect("dismiss"),
        Some(SESSION.to_owned())
    );
    assert_eq!(service.dismiss_member(&plain.id).expect("dismiss"), None);
    assert!(matches!(
        service.dismiss_member(&builder.id),
        Err(TeamError::MemberNotFound(_))
    ));
}

#[test]
fn set_session_attaches_and_clears_a_session_and_keeps_the_worktree() {
    let (service, _dir) = service();
    let builder = add(&service, placed_agent("Builder", WORKTREE, Some(SESSION)));

    service.set_session(&builder.id, None).expect("it closes");
    let closed = get(&service, &builder.id);
    assert_eq!(closed.session_id, None);
    assert_eq!(closed.worktree, WORKTREE, "the worktree stays");

    service
        .set_session(&builder.id, Some(RESTORED))
        .expect("restored in the same worktree");
    assert_eq!(
        get(&service, &builder.id).session_id.as_deref(),
        Some(RESTORED)
    );
    assert!(matches!(
        service.set_session(&builder.id, Some("not a plain id")),
        Err(TeamError::InvalidSession)
    ));
    assert_eq!(
        get(&service, &builder.id).session_id.as_deref(),
        Some(RESTORED),
        "a refused session changes nothing"
    );
    assert!(matches!(
        service.set_session("missing", None),
        Err(TeamError::MemberNotFound(_))
    ));
}

#[test]
fn a_change_to_a_member_keeps_its_worktree_and_session() {
    let (service, _dir) = service();
    let lead = add(&service, agent("Lead", None, true));
    let builder = add(&service, placed_agent("Builder", WORKTREE, Some(SESSION)));

    service.set_role(&builder.id, "Maker").expect("role");
    service
        .set_parent(&builder.id, Some(&lead.id))
        .expect("parent");
    service
        .set_task(&builder.id, "login", "tests pass")
        .expect("task");

    let changed = get(&service, &builder.id);
    assert_eq!(changed.worktree, WORKTREE);
    assert_eq!(changed.session_id.as_deref(), Some(SESSION));
}

#[test]
fn a_worktree_can_have_several_trees_with_their_own_roots() {
    let (service, _dir) = service();
    let first = add(&service, agent("First lead", None, true));
    let second = add(&service, agent("Second lead", None, true));
    let worker = add(&service, agent("Worker", Some(&second.id), false));

    assert_eq!(
        second.parent_id, None,
        "a second root is allowed in the same worktree"
    );
    assert_eq!(worker.parent_id.as_deref(), Some(second.id.as_str()));
    service
        .set_linked(&first.id, &worker.id, true)
        .expect("the trees of one worktree may talk");
    assert_eq!(members_of(&service, WORKTREE).len(), 3);
}

#[test]
fn members_of_two_worktrees_never_pair_or_parent_each_other() {
    let (service, _dir) = service();
    let lead = add(&service, agent("Lead", None, true));
    let writer = add(&service, agent_in(OTHER_WORKTREE, "Writer", None, false));
    let other_lead = add(&service, agent_in(OTHER_WORKTREE, "Lead", None, true));

    assert!(matches!(
        service.set_linked(&lead.id, &writer.id, true),
        Err(TeamError::OtherWorktree)
    ));
    assert!(matches!(
        service.set_parent(&writer.id, Some(&lead.id)),
        Err(TeamError::OtherWorktree)
    ));
    assert!(matches!(
        service.add_member(agent_in(OTHER_WORKTREE, "Helper", Some(&lead.id), false)),
        Err(TeamError::OtherWorktree)
    ));
    assert!(matches!(
        service.add_member(agent("Helper", Some(&other_lead.id), false)),
        Err(TeamError::OtherWorktree)
    ));
    assert!(service.snapshot().expect("snapshot").pairs.is_empty());
}

#[test]
fn a_new_root_orchestrator_starts_open_and_every_other_new_member_starts_closed() {
    let (service, _dir) = service();
    let lead = add(&service, agent("Lead", None, true));
    let builder = add(&service, agent("Builder", Some(&lead.id), true));
    let reviewer = add(&service, agent("Reviewer", None, false));
    let docs = add(&service, bot("docs", "Docs"));

    assert!(lead.expanded);
    assert!(
        !builder.expanded,
        "a member under an orchestrator starts closed"
    );
    assert!(!reviewer.expanded);
    assert!(docs.expanded, "a bot is an orchestrator root");
    assert_eq!(get(&service, &lead.id).expanded, lead.expanded);
}

#[test]
fn set_expanded_is_read_back_from_the_snapshot_and_an_unknown_member_is_refused() {
    let (service, _dir) = service();
    let lead = add(&service, agent("Lead", None, true));

    service.set_expanded(&lead.id, false).expect("close");
    assert!(!get(&service, &lead.id).expanded);
    service.set_expanded(&lead.id, true).expect("open");
    let stored = get(&service, &lead.id);
    assert!(stored.expanded);
    let json = serde_json::to_value(&stored).expect("json");
    assert_eq!(json["expanded"], true);
    assert!(matches!(
        service.set_expanded("missing", true),
        Err(TeamError::MemberNotFound(_))
    ));
}

#[test]
fn startup_removes_a_bot_the_checker_rejects_with_its_subtree_and_lines_in_every_worktree() {
    let (service, _dir) = service();
    let lead = add(&service, agent("Lead", None, true));
    let docs = add(&service, bot("docs", "Docs"));
    let ghost = add(&service, bot("ghost", "Ghost"));
    let ghost_elsewhere = add(&service, bot_in(OTHER_WORKTREE, "ghost", "Ghost"));
    let helper = add(&service, agent("Helper", Some(&ghost.id), true));
    let tester = add(&service, agent("Tester", Some(&helper.id), false));
    add(
        &service,
        agent_in(OTHER_WORKTREE, "Writer", Some(&ghost_elsewhere.id), false),
    );
    service
        .set_linked(&tester.id, &lead.id, true)
        .expect("link");
    service.set_linked(&docs.id, &lead.id, true).expect("link");
    let lead_before = get(&service, &lead.id);

    let removed = service
        .remove_missing_bots(|bot_id| bot_id == "docs")
        .expect("reconcile");

    // Both ghosts, the helper and tester under the first, and the writer under the second.
    assert_eq!(removed, 5);
    let snapshot = service.snapshot().expect("snapshot");
    let left: Vec<String> = snapshot
        .members
        .iter()
        .map(|member| member.id.clone())
        .collect();
    assert_eq!(left, [lead.id.clone(), docs.id.clone()]);
    assert_eq!(snapshot.pairs, [TeamPair::between(&docs.id, &lead.id)]);
    assert_eq!(
        get(&service, &lead.id),
        lead_before,
        "the agent stays as it was"
    );
    assert_eq!(
        service
            .remove_missing_bots(|bot_id| bot_id == "docs")
            .expect("again"),
        0
    );
}

#[test]
fn startup_leaves_existing_bots_and_every_agent_as_they_were() {
    let (service, _dir) = service();
    let lead = add(&service, agent("Lead", None, true));
    let builder = add(&service, agent("Builder", Some(&lead.id), true));
    let worker = add(&service, agent("Worker", Some(&builder.id), false));
    let docs = add(&service, bot("docs", "Docs"));
    service
        .set_linked(&docs.id, &worker.id, true)
        .expect("link");
    service.set_expanded(&builder.id, true).expect("open");
    let before = service.snapshot().expect("snapshot");

    let removed = service
        .remove_missing_bots(|bot_id| bot_id == "docs")
        .expect("reconcile");

    assert_eq!(removed, 0);
    assert_eq!(service.snapshot().expect("snapshot"), before);
}
