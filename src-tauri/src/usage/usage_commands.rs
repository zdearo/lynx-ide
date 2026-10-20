use std::sync::Arc;

use tauri::State;

use crate::blocking_task::run_blocking;
use crate::config::ConfigStore;
use crate::error::IpcError;

use super::plan_usage::PlanUsage;
use super::plan_usage_service::PlanUsageService;

/// The plan limits the settings ask for. Nothing is read while the footer's limits are off.
/// `force` skips the cache: used on refresh.
#[tauri::command]
#[specta::specta]
pub async fn get_plan_usage(
    service: State<'_, PlanUsageService>,
    config: State<'_, Arc<ConfigStore>>,
    force: bool,
) -> Result<Vec<PlanUsage>, IpcError> {
    let service = service.inner().clone();
    let usage = config.config().usage;
    run_blocking(move || {
        if !usage.visible {
            return Ok(Vec::new());
        }
        Ok(service.snapshot(&usage.providers, force))
    })
    .await
}
