//! The repositories a bot may read. The person registers a repository in Projects, the bot asks to
//! read it, and the person approves in the chat. A grant covers the repository and its worktrees.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use specta::Type;

/// A repository registered in Lynx, as a bot may name it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoRef {
    pub id: String,
    pub name: String,
}

/// What Lynx knows about the registered repositories. The app implements it; tests use a fake.
pub trait RegisteredRepos: Send + Sync {
    fn repos(&self) -> Vec<RepoRef>;
    /// The main folder of a repository and the folder of each worktree. Empty for an unknown id.
    fn folders_of(&self, repo_id: &str) -> Vec<PathBuf>;
}

/// A repository the person let a bot read. The name is kept from the approval, for the screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Grant {
    pub id: String,
    pub name: String,
}

/// Where an access request stands. A request is answered once: pending becomes approved or denied.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum AccessState {
    #[default]
    Pending,
    Approved,
    Denied,
}

/// What a bot asks the person for. The card in the chat is the same for every kind: it lists the
/// items, and the person approves or refuses. Only what approving does depends on the kind.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum RequestKind {
    /// Read a registered project.
    #[default]
    Project,
    /// Run commands that match patterns like `minha-cli *`.
    Commands,
    /// Start agents, which run without asking for permission, in a registered project.
    Workers,
}

/// A bot's request, as the chat shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct AccessRequest {
    pub kind: RequestKind,
    /// What is asked: the project's name, or the command patterns.
    pub items: Vec<String>,
    /// The project asked for, when the kind is `project`.
    pub repo_id: Option<String>,
    pub state: AccessState,
}

impl AccessRequest {
    /// Whether `other` asks for the same thing, so a second one is not needed while this one waits.
    pub fn same_ask(&self, other: &Self) -> bool {
        let sorted = |request: &Self| {
            let mut items = request.items.clone();
            items.sort();
            items
        };
        self.kind == other.kind && self.repo_id == other.repo_id && sorted(self) == sorted(other)
    }
}

impl Default for AccessRequest {
    fn default() -> Self {
        Self {
            kind: RequestKind::Project,
            items: Vec::new(),
            repo_id: None,
            state: AccessState::Pending,
        }
    }
}

/// The repository a bot named: by its id, or by its name when no id matches.
pub fn find_repo(repos: &[RepoRef], query: &str) -> Option<RepoRef> {
    let query = query.trim();
    repos
        .iter()
        .find(|repo| repo.id == query)
        .or_else(|| repos.iter().find(|repo| repo.name == query))
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repos() -> Vec<RepoRef> {
        vec![
            RepoRef {
                id: "r1".to_owned(),
                name: "lynx".to_owned(),
            },
            RepoRef {
                id: "r2".to_owned(),
                name: "site".to_owned(),
            },
        ]
    }

    #[test]
    fn a_repo_is_found_by_its_id_or_its_name_and_nothing_else_is() {
        assert_eq!(
            find_repo(&repos(), "r2").map(|r| r.name),
            Some("site".to_owned())
        );
        assert_eq!(
            find_repo(&repos(), " lynx ").map(|r| r.id),
            Some("r1".to_owned())
        );
        assert_eq!(find_repo(&repos(), "outro"), None);
    }
}
