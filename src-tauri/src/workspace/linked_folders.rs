use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::PoisonError;

use serde::Serialize;
use specta::Type;
use uuid::Uuid;

use crate::error::AppError;

use super::linked_folder_scan::{is_repository, scan};
use super::linked_folder_sync::{PlanInput, canonical_key, plan, without_verbatim_prefix};
use super::workspace_models::Folder;
use super::workspace_service::{WorkspaceService, path_to_string, validate_folder_name};

#[derive(Debug, thiserror::Error)]
pub enum LinkedFolderError {
    #[error("{0} is a git repository; add it as a project instead")]
    IsRepository(String),
    #[error("{0} overlaps a folder that is already linked")]
    Overlaps(String),
    #[error("this folder mirrors a folder on disk; unlink the folder it belongs to first")]
    Managed,
    #[error("this folder is not linked to a folder on disk")]
    NotLinked,
}

/// What linking a directory would bring in, shown before anything is created.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LinkedFolderPreview {
    /// Folders that would be made under the linked one.
    pub folders: u32,
    pub projects: u32,
    /// A limit cut the scan short, so there may be more.
    pub truncated: bool,
}

impl WorkspaceService {
    /// Counts what `path` holds without creating or changing anything.
    pub fn preview_linked_folder(&self, path: &str) -> Result<LinkedFolderPreview, AppError> {
        let root = self.validate_link_target(path)?;
        let tree = scan(&root);
        Ok(LinkedFolderPreview {
            folders: u32::try_from(tree.directories.len()).unwrap_or(u32::MAX),
            projects: u32::try_from(tree.repos.len()).unwrap_or(u32::MAX),
            truncated: tree.truncated,
        })
    }

    /// Creates a folder that mirrors `path`: the directories under it become folders and the first
    /// git repository on each branch becomes a project. Nothing on disk is touched.
    pub fn link_folder(
        &self,
        name: &str,
        parent_id: Option<&str>,
        path: &str,
    ) -> Result<Folder, AppError> {
        let name = validate_folder_name(name)?;
        if let Some(parent) = parent_id {
            self.find_folder(parent)?;
        }
        let _sync = self
            .linked_sync
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let root_path = self.validate_link_target(path)?;

        let id = Uuid::new_v4().to_string();
        let folder = Folder {
            id: id.clone(),
            name,
            parent_id: parent_id.map(str::to_owned),
            collapsed: false,
            source_path: Some(path_to_string(&root_path)),
            linked_root_id: Some(id),
            missing: false,
        };
        self.database.insert_folder(&folder)?;
        self.sync_root(&folder)?;
        Ok(folder)
    }

    /// Reads the linked directories again and adds what is new, or all of them when `only` is
    /// `None`. Returns whether anything changed, so the sidebar knows to reload.
    pub fn sync_linked_folders(&self, only: Option<&str>) -> Result<bool, AppError> {
        let _sync = self
            .linked_sync
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let roots: Vec<Folder> = self
            .database
            .list_folders()?
            .into_iter()
            .filter(|folder| folder.is_linked_root())
            .filter(|folder| only.is_none_or(|id| id == folder.id))
            .collect();
        if let Some(id) = only
            && roots.is_empty()
        {
            return Err(AppError::FolderNotFound(id.to_owned()));
        }

        let mut changed = false;
        for root in &roots {
            changed |= self.sync_root(root)?;
        }
        Ok(changed)
    }

    /// The linked folder and everything under it become ordinary folders and projects.
    pub fn unlink_folder(&self, folder_id: &str) -> Result<(), AppError> {
        let _sync = self
            .linked_sync
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let folder = self.find_folder(folder_id)?;
        if !folder.is_linked_root() {
            return Err(LinkedFolderError::NotLinked.into());
        }
        self.database.unlink_folder(folder_id)?;
        Ok(())
    }

    /// Callers hold the sync lock, so two syncs never plan the same folder twice.
    fn sync_root(&self, root: &Folder) -> Result<bool, AppError> {
        let Some(source) = root.source_path.as_deref() else {
            return Ok(false);
        };
        let source = PathBuf::from(source);
        let tree = source.is_dir().then(|| scan(&source));

        let folders = self.database.list_folders()?;
        let repos = self.database.list_repos()?;
        let taken: HashSet<String> = repos
            .iter()
            .map(|repo| canonical_key(Path::new(&repo.path)))
            .collect();
        let excluded = self.database.linked_exclusions(&root.id)?;

        let plan = plan(&PlanInput {
            root,
            scan: tree.as_ref(),
            folders: &folders,
            repos: &repos,
            taken: &taken,
            excluded: &excluded,
        });
        if plan.is_empty() {
            return Ok(false);
        }
        self.database.apply_linked_plan(&plan)?;
        Ok(true)
    }

    /// The canonical directory to mirror, or why it cannot be mirrored.
    fn validate_link_target(&self, path: &str) -> Result<PathBuf, AppError> {
        let given = Path::new(path);
        if !given.exists() {
            return Err(AppError::LocationNotFound(path.to_owned()));
        }
        if !given.is_dir() {
            return Err(AppError::LocationNotADirectory(path.to_owned()));
        }
        let root = fs::canonicalize(given)
            .map(without_verbatim_prefix)
            .map_err(|_| AppError::LocationNotFound(path.to_owned()))?;
        if is_repository(&root) || root.join(".git").exists() {
            return Err(LinkedFolderError::IsRepository(path_to_string(&root)).into());
        }

        for folder in self.database.list_folders()? {
            if !folder.is_linked_root() {
                continue;
            }
            let Some(linked) = folder.source_path.as_deref().map(Path::new) else {
                continue;
            };
            if root.starts_with(linked) || linked.starts_with(&root) {
                return Err(LinkedFolderError::Overlaps(path_to_string(&root)).into());
            }
        }
        Ok(root)
    }
}
