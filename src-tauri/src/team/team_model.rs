//! The team's data: runs, their members, and the lines of conversation between members. A run is
//! one goal with its own team; nothing links two runs.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::agents::harness::HarnessId;

/// A CLI agent in a worktree, or a Lynx bot. A bot only orchestrates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum TeamMemberKind {
    Agent,
    Bot,
}

impl TeamMemberKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Bot => "bot",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "agent" => Some(Self::Agent),
            "bot" => Some(Self::Bot),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TeamRun {
    pub id: String,
    /// The run's goal. It is not a task of any member.
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TeamMember {
    pub id: String,
    pub run_id: String,
    pub kind: TeamMemberKind,
    pub role: String,
    /// The agent's harness. Only agents have one.
    pub harness: Option<HarnessId>,
    /// The Lynx bot. Only bots have one.
    pub bot_id: Option<String>,
    /// The orchestrator this member answers to. None means it belongs to none.
    pub parent_id: Option<String>,
    /// Whether it may have members under it. A bot always is.
    pub orchestrator: bool,
    /// Only a member with an orchestrator has a task and a "done when" line; otherwise both are empty.
    pub task: String,
    pub acceptance: String,
}

/// A permission for two members to talk. Stored once, with the lower id first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TeamPair {
    pub a: String,
    pub b: String,
}

impl TeamPair {
    pub fn between(first: &str, second: &str) -> Self {
        if first <= second {
            Self {
                a: first.to_owned(),
                b: second.to_owned(),
            }
        } else {
            Self {
                a: second.to_owned(),
                b: first.to_owned(),
            }
        }
    }
}

/// Everything the team screen shows, in the order it was made.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TeamSnapshot {
    pub runs: Vec<TeamRun>,
    pub members: Vec<TeamMember>,
    pub pairs: Vec<TeamPair>,
}

/// A member to add. An agent may start under an orchestrator; a bot is always an orchestrator under no one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum NewTeamMember {
    Agent {
        role: String,
        harness: HarnessId,
        parent_id: Option<String>,
        orchestrator: bool,
        task: String,
        acceptance: String,
    },
    Bot {
        bot_id: String,
        /// The bot's name in the run.
        role: String,
    },
}
