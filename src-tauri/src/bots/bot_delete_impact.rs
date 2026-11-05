//! What deleting a bot takes with it, read before the owner confirms: the team members the removal drops and
//! the workers it stops. Nothing here changes anything.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::error::AppError;
use crate::orchestration::service::Orchestration;
use crate::team::team_service::TeamService;

/// What deleting a bot takes with it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DeleteImpact {
    /// The bot's members in every team, then the members under them, as the removal takes them.
    pub members: Vec<ImpactMember>,
    /// The workers the bot started that still run; deleting the bot stops them.
    pub running_workers: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImpactMember {
    pub role: String,
    /// How many members sit directly under this one.
    pub under: u32,
}

/// What deleting `bot` takes with it. Workers are counted the way the deletion stops them.
pub fn delete_impact(
    team: &TeamService,
    orchestration: &Orchestration,
    bot: &str,
) -> Result<DeleteImpact, AppError> {
    let members = team.bot_members(bot)?;
    let listed = members
        .iter()
        .map(|member| ImpactMember {
            role: member.role.clone(),
            under: count(
                members
                    .iter()
                    .filter(|other| other.parent_id.as_deref() == Some(member.id.as_str()))
                    .count(),
            ),
        })
        .collect();
    let running = orchestration
        .workers_of_bot(bot)?
        .iter()
        .filter(|worker| worker.running)
        .count();
    Ok(DeleteImpact {
        members: listed,
        running_workers: count(running),
    })
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::agents::harness::HarnessId;
    use crate::orchestration::service::{Orchestration, WorkerHost, WorkerWorktree};
    use crate::orchestration::service_tests::FakeHost;
    use crate::persistence::AppDatabase;
    use crate::team::team_model::{NewTeamMember, TeamMember};
    use crate::team::team_service::TeamService;

    use super::delete_impact;

    /// A bot `docs` in the team of `/repo/app`, with a member under it that its bot started a worker for, which still runs.
    struct Fixture {
        _dir: tempfile::TempDir,
        orchestration: Orchestration,
        team: Arc<TeamService>,
        bot: TeamMember,
    }

    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().expect("temp dir");
        let database = Arc::new(AppDatabase::open(&dir.path().join("app.db")).expect("database"));
        let team = Arc::new(TeamService::new(Arc::clone(&database), Box::new(|_| true)));
        let orchestration = Orchestration::new(
            Arc::clone(&team),
            Arc::new(FakeHost::new()) as Arc<dyn WorkerHost>,
        );
        let bot = team
            .add_member(NewTeamMember::Bot {
                bot_id: "docs".to_owned(),
                role: "Docs".to_owned(),
                worktree: "/repo/app".to_owned(),
            })
            .expect("the bot joins the team");
        let writer = orchestration
            .add_member("bot:docs:t1", None, "Writer", "Audite", "", Vec::new())
            .expect("the bot adds a member");
        orchestration
            .start_worker(
                "bot:docs:t1",
                &writer.id,
                HarnessId::Claude,
                WorkerWorktree::Current,
                None,
            )
            .expect("the bot starts a worker that keeps running");
        Fixture {
            _dir: dir,
            orchestration,
            team,
            bot,
        }
    }

    #[test]
    fn the_impact_names_the_members_and_counts_the_running_workers() {
        let fixture = fixture();

        let impact = delete_impact(&fixture.team, &fixture.orchestration, "docs").expect("impact");

        assert_eq!(impact.running_workers, 1);
        assert_eq!(impact.members.len(), 2, "the bot and the member under it");
        let bot = impact
            .members
            .iter()
            .find(|member| member.role == fixture.bot.role)
            .expect("the bot is listed");
        assert_eq!(bot.under, 1);
    }

    #[test]
    fn reading_the_impact_changes_nothing() {
        let fixture = fixture();

        delete_impact(&fixture.team, &fixture.orchestration, "docs").expect("impact");

        assert_eq!(fixture.team.bot_members("docs").expect("members").len(), 2);
    }
}
