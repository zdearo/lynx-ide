//! The team's changes against a real database in a temporary folder: each rule, its refusal, and what a
//! change leaves behind.

use std::sync::Arc;

use crate::agents::harness::HarnessId;
use crate::persistence::AppDatabase;

use super::team_error::TeamError;
use super::team_model::{NewTeamMember, TeamMember, TeamMemberKind};
use super::team_service::TeamService;

fn service() -> (TeamService, tempfile::TempDir) {
    service_with_bots(|_| true)
}

/// A service where the bot ids `bot_exists` accepts are the bots that exist.
fn service_with_bots(bot_exists: fn(&str) -> bool) -> (TeamService, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let database = AppDatabase::open(&dir.path().join("app.db")).expect("open");
    (
        TeamService::new(Arc::new(database), Box::new(bot_exists)),
        dir,
    )
}

/// An agent with a task and a "done when" line. Under no parent they are dropped.
fn agent(role: &str, parent: Option<&str>, orchestrator: bool) -> NewTeamMember {
    NewTeamMember::Agent {
        role: role.to_owned(),
        harness: HarnessId::Codex,
        parent_id: parent.map(str::to_owned),
        orchestrator,
        task: "write the form".to_owned(),
        acceptance: "the form submits".to_owned(),
    }
}

fn bot(bot_id: &str, role: &str) -> NewTeamMember {
    NewTeamMember::Bot {
        bot_id: bot_id.to_owned(),
        role: role.to_owned(),
    }
}

/// The members of a run, in the order they were added.
fn members_of(service: &TeamService, run_id: &str) -> Vec<TeamMember> {
    service
        .snapshot()
        .expect("snapshot")
        .members
        .into_iter()
        .filter(|member| member.run_id == run_id)
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

/// The first member of a run: the lead it started with.
fn lead_of(service: &TeamService, run_id: &str) -> String {
    members_of(service, run_id).remove(0).id
}

#[test]
fn create_run_trims_the_goal_and_starts_with_a_claude_lead() {
    let (service, _dir) = service();

    let run = service
        .create_run("  Auth refactor  ", None)
        .expect("create");

    assert_eq!(run.title, "Auth refactor");
    let lead = members_of(&service, &run.id).remove(0);
    assert_eq!(lead.role, "Lead");
    assert_eq!(lead.kind, TeamMemberKind::Agent);
    assert_eq!(lead.harness, Some(HarnessId::Claude));
    assert!(lead.orchestrator);
    assert_eq!(lead.parent_id, None);
}

#[test]
fn create_run_with_a_bot_starts_with_that_bot() {
    let (service, _dir) = service();

    let run = service.create_run("Docs", Some("docs")).expect("create");

    let lead = members_of(&service, &run.id).remove(0);
    assert_eq!(lead.kind, TeamMemberKind::Bot);
    assert_eq!(lead.bot_id.as_deref(), Some("docs"));
    assert_eq!(lead.harness, None);
    assert!(lead.orchestrator);
    assert!(matches!(
        service.create_run("Other", Some("Not A Bot")),
        Err(TeamError::InvalidBot)
    ));
}

#[test]
fn create_run_refuses_an_empty_goal_and_one_over_80_characters() {
    let (service, _dir) = service();

    assert!(matches!(
        service.create_run("   ", None),
        Err(TeamError::InvalidTitle)
    ));
    assert!(matches!(
        service.create_run(&"x".repeat(81), None),
        Err(TeamError::InvalidTitle)
    ));
    assert!(service.create_run(&"x".repeat(80), None).is_ok());
}

#[test]
fn runs_come_back_in_the_order_they_were_made() {
    let (service, _dir) = service();
    service.create_run("Auth", None).expect("create");
    service.create_run("Docs", None).expect("create");

    let titles: Vec<String> = service
        .snapshot()
        .expect("snapshot")
        .runs
        .into_iter()
        .map(|run| run.title)
        .collect();

    assert_eq!(titles, ["Auth", "Docs"]);
}

#[test]
fn add_member_refuses_a_parent_that_is_not_an_orchestrator() {
    let (service, _dir) = service();
    let run = service.create_run("Auth", None).expect("create");
    let lead = lead_of(&service, &run.id);
    let worker = service
        .add_member(&run.id, agent("Worker", Some(&lead), false))
        .expect("add");

    assert!(matches!(
        service.add_member(&run.id, agent("Tester", Some(&worker.id), false)),
        Err(TeamError::NotAnOrchestrator)
    ));
}

#[test]
fn add_member_keeps_its_task_only_under_an_orchestrator() {
    let (service, _dir) = service();
    let run = service.create_run("Auth", None).expect("create");
    let lead = lead_of(&service, &run.id);

    let builder = service
        .add_member(&run.id, agent("Builder", Some(&lead), true))
        .expect("add");
    let solo = service
        .add_member(&run.id, agent("Reviewer", None, false))
        .expect("add");

    assert_eq!(builder.parent_id.as_deref(), Some(lead.as_str()));
    assert_eq!(builder.task, "write the form");
    assert_eq!(solo.parent_id, None);
    assert_eq!(solo.task, "");
    assert_eq!(solo.acceptance, "");
}

#[test]
fn add_member_refuses_an_empty_or_long_role_and_trims_the_role() {
    let (service, _dir) = service();
    let run = service.create_run("Auth", None).expect("create");

    assert!(matches!(
        service.add_member(&run.id, agent("   ", None, false)),
        Err(TeamError::InvalidRole)
    ));
    assert!(matches!(
        service.add_member(&run.id, agent(&"x".repeat(41), None, false)),
        Err(TeamError::InvalidRole)
    ));
    let tester = service
        .add_member(&run.id, agent("  Tester  ", None, false))
        .expect("add");
    assert_eq!(tester.role, "Tester");
}

#[test]
fn add_member_refuses_an_unknown_run_and_a_parent_from_another_run() {
    let (service, _dir) = service();
    let auth = service.create_run("Auth", None).expect("create");
    let docs = service.create_run("Docs", None).expect("create");
    let docs_lead = lead_of(&service, &docs.id);

    assert!(matches!(
        service.add_member("missing", agent("Worker", None, false)),
        Err(TeamError::RunNotFound(_))
    ));
    assert!(matches!(
        service.add_member(&auth.id, agent("Worker", Some(&docs_lead), false)),
        Err(TeamError::OtherRun)
    ));
}

#[test]
fn set_parent_refuses_a_cycle_and_a_member_under_itself_and_changes_nothing() {
    let (service, _dir) = service();
    let run = service.create_run("Auth", None).expect("create");
    let lead = lead_of(&service, &run.id);
    let builder = service
        .add_member(&run.id, agent("Builder", Some(&lead), true))
        .expect("add");

    assert!(matches!(
        service.set_parent(&lead, Some(&builder.id)),
        Err(TeamError::Cycle)
    ));
    assert!(matches!(
        service.set_parent(&builder.id, Some(&builder.id)),
        Err(TeamError::SameMember)
    ));
    assert_eq!(get(&service, &lead).parent_id, None);
    assert_eq!(
        get(&service, &builder.id).parent_id.as_deref(),
        Some(lead.as_str())
    );
}

#[test]
fn set_parent_refuses_a_bot_as_a_child() {
    let (service, _dir) = service();
    let run = service.create_run("Docs", Some("docs")).expect("create");
    let bot_lead = lead_of(&service, &run.id);
    let helper = service
        .add_member(&run.id, agent("Helper", None, true))
        .expect("add");

    assert!(matches!(
        service.set_parent(&bot_lead, Some(&helper.id)),
        Err(TeamError::BotHasParent)
    ));
}

#[test]
fn set_parent_none_detaches_a_member_and_clears_its_task() {
    let (service, _dir) = service();
    let run = service.create_run("Auth", None).expect("create");
    let lead = lead_of(&service, &run.id);
    let builder = service
        .add_member(&run.id, agent("Builder", Some(&lead), true))
        .expect("add");

    service.set_parent(&builder.id, None).expect("detach");

    let detached = get(&service, &builder.id);
    assert_eq!(detached.parent_id, None);
    assert_eq!(detached.task, "");
    assert_eq!(detached.acceptance, "");
    // Attaching it again does not bring the old task back.
    service
        .set_parent(&builder.id, Some(&lead))
        .expect("attach");
    assert_eq!(get(&service, &builder.id).task, "");
}

#[test]
fn set_parent_drops_the_line_between_the_parent_and_the_child() {
    let (service, _dir) = service();
    let run = service.create_run("Auth", None).expect("create");
    let lead = lead_of(&service, &run.id);
    let reviewer = service
        .add_member(&run.id, agent("Reviewer", None, false))
        .expect("add");
    service.set_linked(&lead, &reviewer.id, true).expect("link");
    assert_eq!(service.snapshot().expect("snapshot").pairs.len(), 1);

    service
        .set_parent(&reviewer.id, Some(&lead))
        .expect("attach");

    assert!(service.snapshot().expect("snapshot").pairs.is_empty());
}

#[test]
fn an_orchestrator_with_members_under_it_cannot_stop_being_one() {
    let (service, _dir) = service();
    let run = service.create_run("Auth", None).expect("create");
    let lead = lead_of(&service, &run.id);
    let builder = service
        .add_member(&run.id, agent("Builder", Some(&lead), false))
        .expect("add");

    assert!(matches!(
        service.set_orchestrator(&lead, false),
        Err(TeamError::HasMembers)
    ));
    service.set_parent(&builder.id, None).expect("detach");
    service
        .set_orchestrator(&lead, false)
        .expect("no members left under it");
    assert!(!get(&service, &lead).orchestrator);
}

#[test]
fn a_bot_stays_an_orchestrator() {
    let (service, _dir) = service();
    let run = service.create_run("Docs", Some("docs")).expect("create");
    let bot_lead = lead_of(&service, &run.id);

    assert!(matches!(
        service.set_orchestrator(&bot_lead, false),
        Err(TeamError::BotNotOrchestrator)
    ));
    assert!(get(&service, &bot_lead).orchestrator);
}

#[test]
fn a_bot_joins_each_run_once() {
    let (service, _dir) = service();
    let run = service.create_run("Docs", Some("docs")).expect("create");

    assert!(matches!(
        service.add_member(&run.id, bot("docs", "Docs")),
        Err(TeamError::BotInRun)
    ));
    service
        .create_run("Other docs", Some("docs"))
        .expect("the same bot joins another run");
}

#[test]
fn a_bot_that_does_not_exist_cannot_join_a_run() {
    let (service, _dir) = service_with_bots(|id| id == "docs");
    let run = service.create_run("Docs", None).expect("create");

    assert!(matches!(
        service.add_member(&run.id, bot("ghost", "Ghost")),
        Err(TeamError::UnknownBot)
    ));
    assert!(matches!(
        service.create_run("Ghost", Some("ghost")),
        Err(TeamError::UnknownBot)
    ));
    assert_eq!(members_of(&service, &run.id).len(), 1, "only the lead");
    assert_eq!(service.snapshot().expect("snapshot").runs.len(), 1);
}

#[test]
fn an_existing_bot_joins_a_run() {
    let (service, _dir) = service_with_bots(|id| id == "docs");
    let run = service.create_run("Docs", None).expect("create");

    let member = service
        .add_member(&run.id, bot("docs", "Docs"))
        .expect("the bot exists");

    assert_eq!(member.bot_id.as_deref(), Some("docs"));
    assert_eq!(members_of(&service, &run.id).len(), 2);
}

#[test]
fn set_role_changes_an_agent_and_refuses_a_bot() {
    let (service, _dir) = service();
    let run = service.create_run("Docs", Some("docs")).expect("create");
    let bot_lead = lead_of(&service, &run.id);
    let writer = service
        .add_member(&run.id, agent("Writer", None, false))
        .expect("add");

    service.set_role(&writer.id, "  Editor  ").expect("role");
    assert_eq!(get(&service, &writer.id).role, "Editor");
    assert!(matches!(
        service.set_role(&writer.id, ""),
        Err(TeamError::InvalidRole)
    ));
    assert!(matches!(
        service.set_role(&bot_lead, "Boss"),
        Err(TeamError::BotRole)
    ));
}

#[test]
fn set_task_needs_an_orchestrator_above_the_member() {
    let (service, _dir) = service();
    let run = service.create_run("Auth", None).expect("create");
    let lead = lead_of(&service, &run.id);
    let solo = service
        .add_member(&run.id, agent("Solo", None, false))
        .expect("add");
    let builder = service
        .add_member(&run.id, agent("Builder", Some(&lead), true))
        .expect("add");

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
    let auth = service.create_run("Auth", None).expect("create");
    let lead = lead_of(&service, &auth.id);
    let builder = service
        .add_member(&auth.id, agent("Builder", Some(&lead), true))
        .expect("add");
    let reviewer = service
        .add_member(&auth.id, agent("Reviewer", None, false))
        .expect("add");
    let docs = service.create_run("Docs", None).expect("create");
    let writer = service
        .add_member(&docs.id, agent("Writer", None, false))
        .expect("add");

    assert!(matches!(
        service.set_linked(&reviewer.id, &reviewer.id, true),
        Err(TeamError::SameMember)
    ));
    assert!(matches!(
        service.set_linked(&lead, &builder.id, true),
        Err(TeamError::LinkedToParent)
    ));
    assert!(matches!(
        service.set_linked(&reviewer.id, &writer.id, true),
        Err(TeamError::OtherRun)
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
    let run = service.create_run("Docs", Some("docs")).expect("create");
    let bot_lead = lead_of(&service, &run.id);
    let second_bot = service
        .add_member(&run.id, bot("writer-bot", "Writer bot"))
        .expect("add");
    let writer = service
        .add_member(&run.id, agent("Writer", None, false))
        .expect("add");

    assert!(matches!(
        service.set_linked(&bot_lead, &second_bot.id, true),
        Err(TeamError::BotPair)
    ));
    service
        .set_linked(&bot_lead, &writer.id, true)
        .expect("a bot takes messages from an agent");
}

#[test]
fn a_line_is_stored_once_with_the_lower_id_first_and_removing_it_is_always_allowed() {
    let (service, _dir) = service();
    let run = service.create_run("Auth", None).expect("create");
    let lead = lead_of(&service, &run.id);
    let reviewer = service
        .add_member(&run.id, agent("Reviewer", None, false))
        .expect("add");

    service.set_linked(&reviewer.id, &lead, true).expect("link");
    let pairs = service.snapshot().expect("snapshot").pairs;
    assert_eq!(pairs.len(), 1);
    assert!(pairs[0].a < pairs[0].b);

    service
        .set_linked(&lead, &reviewer.id, false)
        .expect("unlink");
    service
        .set_linked(&lead, &reviewer.id, false)
        .expect("unlinking a line that is not there is fine");
    assert!(service.snapshot().expect("snapshot").pairs.is_empty());
}

#[test]
fn dismissing_an_orchestrator_leaves_its_members_without_a_parent_or_task() {
    let (service, _dir) = service();
    let run = service.create_run("Auth", None).expect("create");
    let lead = lead_of(&service, &run.id);
    let builder = service
        .add_member(&run.id, agent("Builder", Some(&lead), true))
        .expect("add");
    let tester = service
        .add_member(&run.id, agent("Tester", Some(&builder.id), false))
        .expect("add");
    let reviewer = service
        .add_member(&run.id, agent("Reviewer", None, false))
        .expect("add");
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
    assert_eq!(members_of(&service, &run.id).len(), 3);
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
fn delete_run_removes_its_members_and_lines_and_keeps_the_others() {
    let (service, _dir) = service();
    let auth = service.create_run("Auth", None).expect("create");
    let auth_lead = lead_of(&service, &auth.id);
    let worker = service
        .add_member(&auth.id, agent("Worker", None, false))
        .expect("add");
    service
        .set_linked(&auth_lead, &worker.id, true)
        .expect("link");
    let docs = service.create_run("Docs", None).expect("create");

    service.delete_run(&auth.id).expect("delete");

    let snapshot = service.snapshot().expect("snapshot");
    assert_eq!(snapshot.runs, vec![docs.clone()]);
    assert!(
        snapshot
            .members
            .iter()
            .all(|member| member.run_id == docs.id)
    );
    assert!(snapshot.pairs.is_empty());
    assert!(matches!(
        service.delete_run(&auth.id),
        Err(TeamError::RunNotFound(_))
    ));
}

#[test]
fn a_change_to_a_member_or_a_parent_that_is_not_there_is_refused() {
    let (service, _dir) = service();
    let run = service.create_run("Auth", None).expect("create");
    let lead = lead_of(&service, &run.id);

    assert!(matches!(
        service.set_role("missing", "Editor"),
        Err(TeamError::MemberNotFound(_))
    ));
    assert!(matches!(
        service.set_parent(&lead, Some("missing")),
        Err(TeamError::MemberNotFound(_))
    ));
    assert!(matches!(
        service.set_linked(&lead, "missing", true),
        Err(TeamError::MemberNotFound(_))
    ));
}

#[test]
fn removing_a_bot_takes_the_members_under_it_and_their_lines_in_every_run() {
    let (service, _dir) = service();
    let auth = service.create_run("Auth", Some("docs")).expect("create");
    let auth_bot = lead_of(&service, &auth.id);
    let builder = service
        .add_member(&auth.id, agent("Builder", Some(&auth_bot), true))
        .expect("add");
    service
        .add_member(&auth.id, agent("Tester", Some(&builder.id), false))
        .expect("add");
    let reviewer = service
        .add_member(&auth.id, agent("Reviewer", None, false))
        .expect("add");
    let writer_bot = service
        .add_member(&auth.id, bot("writer-bot", "Writer bot"))
        .expect("add");
    service
        .set_linked(&builder.id, &reviewer.id, true)
        .expect("link");
    service
        .set_linked(&auth_bot, &reviewer.id, true)
        .expect("link");
    let docs = service.create_run("Docs", Some("docs")).expect("create");
    let docs_bot = lead_of(&service, &docs.id);
    service
        .add_member(&docs.id, agent("Writer", Some(&docs_bot), false))
        .expect("add");

    let removed = service.remove_bot("docs").expect("remove");

    // Auth loses the bot, its builder and the tester under the builder; Docs loses the bot and its writer.
    assert_eq!(removed, 5);
    let snapshot = service.snapshot().expect("snapshot");
    assert_eq!(snapshot.runs.len(), 2, "the runs stay");
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
    let run = service.create_run("Auth", None).expect("create");
    let lead = lead_of(&service, &run.id);
    let builder = service
        .add_member(&run.id, agent("Builder", Some(&lead), true))
        .expect("add");
    let worker = service
        .add_member(&run.id, agent("Worker", Some(&builder.id), false))
        .expect("add");
    let docs = service
        .add_member(&run.id, bot("docs", "Docs"))
        .expect("add");
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
    service.create_run("Auth", Some("docs")).expect("create");
    let before = service.snapshot().expect("snapshot");

    assert_eq!(service.remove_bot("ninguem").expect("remove"), 0);

    assert_eq!(service.snapshot().expect("snapshot"), before);
}
