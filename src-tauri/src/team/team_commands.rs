//! Thin commands for the team. Each one hands its change to the team service, off the async runtime, and
//! tells the screens the team changed once the change is written.

use std::sync::Arc;

use tauri::{AppHandle, State};
use tauri_specta::Event;

use crate::error::{AppError, IpcError};
use crate::process::blocking_task::run_blocking;

use super::team_error::TeamError;
use super::team_events::TeamChanged;
use super::team_model::{NewTeamMember, TeamMember, TeamRun, TeamSnapshot};
use super::team_service::TeamService;

/// Runs a change, and announces it only when it was written.
async fn change<T, F>(app: &AppHandle, task: F) -> Result<T, IpcError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, TeamError> + Send + 'static,
{
    let value = run_blocking(move || task().map_err(AppError::from)).await?;
    let _ = TeamChanged {}.emit(app);
    Ok(value)
}

/// The whole team: its runs, members and lines.
#[tauri::command]
#[specta::specta]
pub async fn team_snapshot(service: State<'_, Arc<TeamService>>) -> Result<TeamSnapshot, IpcError> {
    let service = Arc::clone(service.inner());
    run_blocking(move || service.snapshot().map_err(AppError::from)).await
}

/// Starts a run with its orchestrator at the top: Lead, or the bot `bot_id` when one is given.
#[tauri::command]
#[specta::specta]
pub async fn team_create_run(
    app: AppHandle,
    service: State<'_, Arc<TeamService>>,
    title: String,
    bot_id: Option<String>,
) -> Result<TeamRun, IpcError> {
    let service = Arc::clone(service.inner());
    change(&app, move || service.create_run(&title, bot_id.as_deref())).await
}

#[tauri::command]
#[specta::specta]
pub async fn team_delete_run(
    app: AppHandle,
    service: State<'_, Arc<TeamService>>,
    run_id: String,
) -> Result<(), IpcError> {
    let service = Arc::clone(service.inner());
    change(&app, move || service.delete_run(&run_id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn team_add_member(
    app: AppHandle,
    service: State<'_, Arc<TeamService>>,
    run_id: String,
    member: NewTeamMember,
) -> Result<TeamMember, IpcError> {
    let service = Arc::clone(service.inner());
    change(&app, move || service.add_member(&run_id, member)).await
}

#[tauri::command]
#[specta::specta]
pub async fn team_set_parent(
    app: AppHandle,
    service: State<'_, Arc<TeamService>>,
    member_id: String,
    parent_id: Option<String>,
) -> Result<(), IpcError> {
    let service = Arc::clone(service.inner());
    change(&app, move || {
        service.set_parent(&member_id, parent_id.as_deref())
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn team_set_orchestrator(
    app: AppHandle,
    service: State<'_, Arc<TeamService>>,
    member_id: String,
    orchestrator: bool,
) -> Result<(), IpcError> {
    let service = Arc::clone(service.inner());
    change(&app, move || {
        service.set_orchestrator(&member_id, orchestrator)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn team_set_role(
    app: AppHandle,
    service: State<'_, Arc<TeamService>>,
    member_id: String,
    role: String,
) -> Result<(), IpcError> {
    let service = Arc::clone(service.inner());
    change(&app, move || service.set_role(&member_id, &role)).await
}

#[tauri::command]
#[specta::specta]
pub async fn team_set_task(
    app: AppHandle,
    service: State<'_, Arc<TeamService>>,
    member_id: String,
    task: String,
    acceptance: String,
) -> Result<(), IpcError> {
    let service = Arc::clone(service.inner());
    change(&app, move || {
        service.set_task(&member_id, &task, &acceptance)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn team_dismiss_member(
    app: AppHandle,
    service: State<'_, Arc<TeamService>>,
    member_id: String,
) -> Result<(), IpcError> {
    let service = Arc::clone(service.inner());
    change(&app, move || service.dismiss_member(&member_id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn team_set_linked(
    app: AppHandle,
    service: State<'_, Arc<TeamService>>,
    a: String,
    b: String,
    linked: bool,
) -> Result<(), IpcError> {
    let service = Arc::clone(service.inner());
    change(&app, move || service.set_linked(&a, &b, linked)).await
}
