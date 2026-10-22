//! The screen's calls into the orchestration. The user approves a run's budget here; agents have no
//! tool that does it.

use std::sync::Arc;

use tauri::State;

use crate::blocking_task::run_blocking;
use crate::error::{AppError, IpcError};

use super::model::Budget;
use super::service::Orchestration;

/// The user approves a run's budget. Workers can start only after this.
#[tauri::command]
#[specta::specta]
pub async fn approve_run(
    orchestration: State<'_, Arc<Orchestration>>,
    run_id: String,
    budget: Budget,
) -> Result<(), IpcError> {
    let orchestration = orchestration.inner().clone();
    run_blocking(move || {
        orchestration
            .approve(&run_id, budget)
            .map(|_| ())
            .map_err(AppError::from)
    })
    .await
}
