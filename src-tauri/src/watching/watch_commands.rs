use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use tauri::{AppHandle, State};
use tauri_specta::Event;

use crate::blocking_task::run_blocking;
use crate::error::IpcError;
use crate::workspace::workspace_service::WorkspaceService;

use super::worktree_watcher::{ChangeSink, WorktreeWatcher};

/// The one worktree being watched: the one on screen.
#[derive(Clone, Default)]
pub struct WatchState(Arc<Mutex<Option<(PathBuf, WorktreeWatcher)>>>);

/// Watches this worktree, and stops watching the previous one. `None` only stops. Answers whether
/// it is being watched: a worktree with too many folders, or a system that refuses, is not, and
/// the interface then reads it again on a timer.
#[tauri::command]
#[specta::specta]
pub async fn watch_worktree(
    app: AppHandle,
    workspace: State<'_, WorkspaceService>,
    state: State<'_, WatchState>,
    worktree_path: Option<String>,
) -> Result<bool, IpcError> {
    let workspace = workspace.inner().clone();
    let state = state.inner().clone();
    run_blocking(move || {
        let mut active = state.0.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(reported) = worktree_path else {
            *active = None;
            return Ok(false);
        };
        let root = workspace.resolve_worktree(&reported)?;
        if active.as_ref().is_some_and(|(watched, _)| *watched == root) {
            return Ok(true);
        }
        // Stop the old watch first: two native watchers on one worktree would report twice.
        *active = None;

        let sink: ChangeSink = Arc::new(move |change| {
            let _ = change.emit(&app);
        });
        match WorktreeWatcher::start(&root, reported, sink) {
            Ok(watcher) => {
                *active = Some((root, watcher));
                Ok(true)
            }
            Err(_) => Ok(false),
        }
    })
    .await
}
