//! The team's data: the members of each worktree and the lines of conversation between them. A
//! worktree's team is the agents its terminal tabs show; nothing links two worktrees' teams.

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
pub struct TeamMember {
    pub id: String,
    /// The absolute path of the worktree whose team this member is in. It stays when the session
    /// closes, so a session restored later goes back there.
    pub worktree: String,
    pub kind: TeamMemberKind,
    pub role: String,
    /// The agent's harness. Only agents have one.
    pub harness: Option<HarnessId>,
    /// The Lynx bot. Only bots have one.
    pub bot_id: Option<String>,
    /// The orchestrator this member answers to. None means it is the root of a tree.
    pub parent_id: Option<String>,
    /// Whether it may have members under it. A bot always is.
    pub orchestrator: bool,
    /// Only a member with an orchestrator has a task and a "done when" line; otherwise both are empty.
    pub task: String,
    pub acceptance: String,
    /// The terminal session the agent runs in. Only agents have one, and it is cleared when it closes.
    pub session_id: Option<String>,
    /// Whether the member's branch is open in the team tree. A new root starts open; every other new member
    /// starts closed.
    pub expanded: bool,
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

/// Every worktree's members and lines, in the order they were made. The screen filters by worktree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TeamSnapshot {
    pub members: Vec<TeamMember>,
    pub pairs: Vec<TeamPair>,
}

/// A member to add, in the worktree it joins. An agent may start under an orchestrator of that worktree;
/// a bot is always an orchestrator and never under one.
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
        /// The absolute path of the worktree the agent works in.
        worktree: String,
        /// The terminal session the screen opened for the agent in `worktree`.
        session_id: Option<String>,
    },
    Bot {
        bot_id: String,
        /// The bot's name in the team.
        role: String,
        /// The absolute path of the worktree whose team the bot joins.
        worktree: String,
    },
}
