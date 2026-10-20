use serde::Serialize;
use specta::Type;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Repo {
    pub id: String,
    pub name: String,
    /// Path of the repository's main worktree.
    pub path: String,
    /// The folder the repo sits in; `None` at the top level.
    pub folder_id: Option<String>,
    /// The linked folder that found this repo; `None` for a repo added by hand.
    pub linked_root_id: Option<String>,
    /// A linked repo that the last scan no longer found on disk.
    pub missing: bool,
}

/// A user-made folder that groups repositories and other folders in the sidebar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    pub id: String,
    pub name: String,
    /// `None` for a folder at the top level.
    pub parent_id: Option<String>,
    pub collapsed: bool,
    /// Directory this folder mirrors. Set on a linked folder and on the folders made under it.
    pub source_path: Option<String>,
    /// The linked folder this one belongs to: its own id for the linked folder itself.
    pub linked_root_id: Option<String>,
    /// A mirrored folder that the last scan no longer found on disk.
    pub missing: bool,
}

impl Folder {
    /// True for a folder the user linked to a directory (not the folders made under it).
    pub fn is_linked_root(&self) -> bool {
        self.linked_root_id.as_deref() == Some(self.id.as_str())
    }

    /// True for a folder the scan made under a linked folder.
    pub fn is_mirrored(&self) -> bool {
        self.linked_root_id.is_some() && !self.is_linked_root()
    }
}

/// What the app remembers about a worktree that Git itself does not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeMeta {
    pub base_ref: String,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeInfo {
    pub path: String,
    /// Directory name; the repository name for the main worktree.
    pub name: String,
    /// `None` when HEAD is detached.
    pub branch: Option<String>,
    pub head: Option<String>,
    /// Ref the worktree was created from; unknown for worktrees made outside the app.
    pub base_ref: Option<String>,
    pub is_main: bool,
    pub locked: bool,
    /// The worktree directory is gone; `git worktree prune` would drop the entry.
    pub prunable: bool,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RemoveWorktreeResult {
    pub branch: Option<String>,
    pub branch_deleted: bool,
    /// Why the branch was kept, when it was.
    pub branch_kept_reason: Option<String>,
}
