use std::sync::Arc;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, State};
use tauri_specta::Event;

use crate::blocking_task::run_blocking;
use crate::error::{AppError, IpcError};

use super::browser_service::BrowserService;
use super::engine_files::EngineStatus;
use super::engine_installer::EngineProgress;

/// Sent while the engine downloads and unpacks.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct BrowserEngineProgress {
    pub progress: EngineProgress,
}

#[tauri::command]
#[specta::specta]
pub async fn browser_engine_status(
    service: State<'_, Arc<BrowserService>>,
) -> Result<EngineStatus, IpcError> {
    let service = Arc::clone(&service);
    run_blocking(move || Ok(service.engine_status())).await
}

/// Downloads the engine and puts it in place, reporting progress as events. With `force`, an
/// engine of the same version is installed again.
#[tauri::command]
#[specta::specta]
pub async fn browser_engine_install(
    app: AppHandle,
    service: State<'_, Arc<BrowserService>>,
    force: bool,
) -> Result<EngineStatus, IpcError> {
    let service = Arc::clone(&service);
    run_blocking(move || {
        Ok(service.install_engine(force, &mut |progress| {
            // A failed emit means the window is closing; nobody is left to show it to.
            let _ = BrowserEngineProgress { progress }.emit(&app);
        })?)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn browser_engine_remove(
    service: State<'_, Arc<BrowserService>>,
) -> Result<EngineStatus, IpcError> {
    let service = Arc::clone(&service);
    run_blocking(move || service.remove_engine().map_err(AppError::from)).await
}
