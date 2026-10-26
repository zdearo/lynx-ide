use crate::error::{AppError, IpcError};

/// Runs blocking work (Git, disk) off the async runtime so the UI stays responsive.
pub async fn run_blocking<T, F>(task: F) -> Result<T, IpcError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, AppError> + Send + 'static,
{
    match tauri::async_runtime::spawn_blocking(task).await {
        Ok(result) => result.map_err(IpcError::from),
        Err(error) => Err(AppError::TaskFailed(error.to_string()).into()),
    }
}
