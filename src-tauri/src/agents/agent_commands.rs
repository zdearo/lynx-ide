use std::sync::Arc;

use tauri::State;

use crate::config::config_store::ConfigStore;
use crate::error::IpcError;
use crate::process::blocking_task::run_blocking;

use super::agent_catalog::{AgentEntry, catalog};
use super::harness::HarnessId;
use super::hook_install::HookInstallStatus;
use super::hook_targets::HookTargets;
use super::launch_line::launch_line;

/// Every supported agent: whether it is on this computer, and whether its hooks are in.
#[tauri::command]
#[specta::specta]
pub async fn agent_catalog(targets: State<'_, HookTargets>) -> Result<Vec<AgentEntry>, IpcError> {
    let targets = targets.inner().clone();
    run_blocking(move || Ok(catalog(&targets))).await
}

#[tauri::command]
#[specta::specta]
pub async fn hook_status(
    targets: State<'_, HookTargets>,
    harness: HarnessId,
) -> Result<HookInstallStatus, IpcError> {
    let targets = targets.inner().clone();
    run_blocking(move || Ok(targets.status(harness))).await
}

/// Turns an agent on: writes its hooks and adds the browser tools. The switch in the settings is
/// the user's yes; nothing here runs on its own.
#[tauri::command]
#[specta::specta]
pub async fn install_hooks(
    targets: State<'_, HookTargets>,
    harness: HarnessId,
) -> Result<HookInstallStatus, IpcError> {
    let targets = targets.inner().clone();
    run_blocking(move || Ok(targets.install(harness)?)).await
}

#[tauri::command]
#[specta::specta]
pub async fn remove_hooks(
    targets: State<'_, HookTargets>,
    harness: HarnessId,
) -> Result<HookInstallStatus, IpcError> {
    let targets = targets.inner().clone();
    run_blocking(move || Ok(targets.remove(harness)?)).await
}

/// Saves a long prompt to a file and returns its path, for an agent to be told to read.
#[tauri::command]
#[specta::specta]
pub async fn write_agent_prompt(prompt: String) -> Result<String, IpcError> {
    run_blocking(move || {
        let path = super::agent_prompt_file::write(&prompt)?;
        Ok(path.to_string_lossy().into_owned())
    })
    .await
}

/// The line that starts an agent, with the launch mode and the arguments Settings hold now.
#[tauri::command]
#[specta::specta]
pub fn agent_launch_line(store: State<'_, Arc<ConfigStore>>, harness: HarnessId) -> String {
    let agents = store.config().agents;
    launch_line(harness, agents.launch_mode, &agents.launch_arguments)
}
