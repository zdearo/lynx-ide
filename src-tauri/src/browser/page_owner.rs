//! Who a browser page belongs to. The owner is whoever launched the session that opened the page,
//! not the place that session runs in: a bot that starts an agent inside a project still owns the
//! pages that agent opens.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PageOwner {
    /// A project's worktree: the pages its own terminals and agents open.
    Workspace { worktree_path: String },
    /// A bot, which lives outside any project.
    Bot { id: String },
}

/// Which owner each live terminal session acts for. The terminal manager fills it in as sessions
/// start and stop; the browser tools read it to find the caller's pages.
#[derive(Clone, Default)]
pub struct SessionOwners {
    owners: Arc<Mutex<HashMap<String, PageOwner>>>,
}

impl SessionOwners {
    fn owners(&self) -> MutexGuard<'_, HashMap<String, PageOwner>> {
        self.owners
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn set(&self, session: &str, owner: PageOwner) {
        self.owners().insert(session.to_owned(), owner);
    }

    pub fn remove(&self, session: &str) {
        self.owners().remove(session);
    }

    pub fn get(&self, session: &str) -> Option<PageOwner> {
        self.owners().get(session).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace(path: &str) -> PageOwner {
        PageOwner::Workspace {
            worktree_path: path.to_owned(),
        }
    }

    #[test]
    fn a_session_is_known_until_it_is_removed() {
        let owners = SessionOwners::default();
        assert_eq!(owners.get("s1"), None);
        owners.set("s1", workspace("/a"));
        assert_eq!(owners.get("s1"), Some(workspace("/a")));
        owners.remove("s1");
        assert_eq!(owners.get("s1"), None);
    }

    #[test]
    fn clones_share_one_registry() {
        let owners = SessionOwners::default();
        let other = owners.clone();
        other.set("s1", PageOwner::Bot { id: "b".into() });
        assert_eq!(owners.get("s1"), Some(PageOwner::Bot { id: "b".into() }));
    }

    #[test]
    fn an_owner_is_told_apart_on_the_wire() {
        let json = serde_json::to_string(&workspace("/a")).unwrap();
        assert_eq!(json, r#"{"kind":"workspace","worktreePath":"/a"}"#);
        let json = serde_json::to_string(&PageOwner::Bot { id: "b".into() }).unwrap();
        assert_eq!(json, r#"{"kind":"bot","id":"b"}"#);
    }
}
