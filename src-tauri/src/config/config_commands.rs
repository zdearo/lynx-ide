use std::sync::Arc;

use tauri::State;

use crate::blocking_task::run_blocking;
use crate::error::{AppError, IpcError};

use super::app_config::AppConfig;
use super::config_store::{ConfigError, ConfigSnapshot, ConfigStore};
use super::installed_shells::installed_shells;

#[tauri::command]
#[specta::specta]
pub async fn get_config(store: State<'_, Arc<ConfigStore>>) -> Result<ConfigSnapshot, IpcError> {
    Ok(store.snapshot())
}

/// Shells the settings can offer for new terminals.
#[tauri::command]
#[specta::specta]
pub async fn list_shells() -> Result<Vec<String>, IpcError> {
    run_blocking(|| Ok(installed_shells())).await
}

#[tauri::command]
#[specta::specta]
pub async fn update_config(
    store: State<'_, Arc<ConfigStore>>,
    config: AppConfig,
) -> Result<ConfigSnapshot, IpcError> {
    let store = Arc::clone(store.inner());
    run_blocking(move || Ok(store.update(config)?)).await
}

/// Opens the config file in the user's default editor, creating it first if needed.
#[tauri::command]
#[specta::specta]
pub async fn open_config_file(store: State<'_, Arc<ConfigStore>>) -> Result<(), IpcError> {
    let store = Arc::clone(store.inner());
    run_blocking(move || {
        store.ensure_file()?;
        open::that(store.path()).map_err(|source| {
            AppError::from(ConfigError::Io {
                path: store.path().to_owned(),
                source,
            })
        })
    })
    .await
}
