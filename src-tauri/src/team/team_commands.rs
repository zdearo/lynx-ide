//! Thin commands for the team. Each one hands its change to the team service, off the async runtime, and
//! tells the screens the team changed once the change is written.

use std::sync::Arc;

use tauri::{AppHandle, State};
use tauri_specta::Event;

use crate::error::{AppError, IpcError};
use crate::orchestration::service::Orchestration;
use crate::process::blocking_task::run_blocking;

use super::team_error::TeamError;
use super::team_events::TeamChanged;
use super::team_model::{NewTeamMember, TeamMember, TeamSnapshot};
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

/// Every worktree's team: its members and lines, and the state of each worker among them.
#[tauri::command]
#[specta::specta]
pub async fn team_snapshot(
    service: State<'_, Arc<TeamService>>,
    orchestration: State<'_, Arc<Orchestration>>,
) -> Result<TeamSnapshot, IpcError> {
    let service = Arc::clone(service.inner());
    let orchestration = Arc::clone(orchestration.inner());
    run_blocking(move || {
        let mut snapshot = service.snapshot()?;
        orchestration.fill_worker_states(&mut snapshot)?;
        Ok::<_, AppError>(snapshot)
    })
    .await
}

/// Adds a member to the team of its worktree.
#[tauri::command]
#[specta::specta]
pub async fn team_add_member(
    app: AppHandle,
    service: State<'_, Arc<TeamService>>,
    member: NewTeamMember,
) -> Result<TeamMember, IpcError> {
    let service = Arc::clone(service.inner());
    change(&app, move || service.add_member(member)).await
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

/// Removes a member. Returns the terminal session it ran in, if any, so the screen can close that terminal.
#[tauri::command]
#[specta::specta]
pub async fn team_dismiss_member(
    app: AppHandle,
    service: State<'_, Arc<TeamService>>,
    member_id: String,
) -> Result<Option<String>, IpcError> {
    let service = Arc::clone(service.inner());
    change(&app, move || service.dismiss_member(&member_id)).await
}

/// Sets what a member waits for. Only a member whose task has not started may change it.
#[tauri::command]
#[specta::specta]
pub async fn team_set_dependencies(
    app: AppHandle,
    service: State<'_, Arc<TeamService>>,
    member_id: String,
    depends_on: Vec<String>,
) -> Result<(), IpcError> {
    let service = Arc::clone(service.inner());
    change(&app, move || {
        service.set_dependencies(&member_id, &depends_on)
    })
    .await
}

/// Attaches the terminal session to an agent, or detaches it when `session_id` is None.
#[tauri::command]
#[specta::specta]
pub async fn team_set_session(
    app: AppHandle,
    service: State<'_, Arc<TeamService>>,
    member_id: String,
    session_id: Option<String>,
) -> Result<(), IpcError> {
    let service = Arc::clone(service.inner());
    change(&app, move || {
        service.set_session(&member_id, session_id.as_deref())
    })
    .await
}

/// Opens or closes a member's branch in the team tree.
#[tauri::command]
#[specta::specta]
pub async fn team_set_expanded(
    app: AppHandle,
    service: State<'_, Arc<TeamService>>,
    member_id: String,
    expanded: bool,
) -> Result<(), IpcError> {
    let service = Arc::clone(service.inner());
    change(&app, move || service.set_expanded(&member_id, expanded)).await
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
