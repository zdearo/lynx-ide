use std::sync::Arc;

use tauri::{AppHandle, State, WebviewWindow};

use crate::blocking_task::run_blocking;
use crate::error::{AppError, IpcError};

use super::browser_error::BrowserError;
use super::browser_service::BrowserService;
use super::browser_state::{BrowserState, BrowserStatus};
use super::detached_window;

/// Starts the browser engine if needed and optionally opens an address. The engine's start-up
/// blocks for a moment, so this runs off the async runtime.
#[tauri::command]
#[specta::specta]
pub async fn browser_open(
    service: State<'_, Arc<BrowserService>>,
    url: Option<String>,
) -> Result<BrowserState, IpcError> {
    if cfg!(not(any(target_os = "linux", windows))) {
        return Err(AppError::from(BrowserError::Unsupported).into());
    }
    let service = Arc::clone(&service);
    run_blocking(move || Ok(service.open(url.as_deref())?)).await
}

#[tauri::command]
#[specta::specta]
pub async fn browser_navigate(
    service: State<'_, Arc<BrowserService>>,
    url: String,
) -> Result<(), IpcError> {
    service
        .navigate(&url)
        .map_err(|error| AppError::from(error).into())
}

#[tauri::command]
#[specta::specta]
pub async fn browser_back(service: State<'_, Arc<BrowserService>>) -> Result<(), IpcError> {
    send(&service, lynx_browser_protocol::message::ToHost::Back)
}

#[tauri::command]
#[specta::specta]
pub async fn browser_forward(service: State<'_, Arc<BrowserService>>) -> Result<(), IpcError> {
    send(&service, lynx_browser_protocol::message::ToHost::Forward)
}

#[tauri::command]
#[specta::specta]
pub async fn browser_reload(
    service: State<'_, Arc<BrowserService>>,
    ignore_cache: bool,
) -> Result<(), IpcError> {
    send(
        &service,
        lynx_browser_protocol::message::ToHost::Reload { ignore_cache },
    )
}

#[tauri::command]
#[specta::specta]
pub async fn browser_stop(service: State<'_, Arc<BrowserService>>) -> Result<(), IpcError> {
    send(&service, lynx_browser_protocol::message::ToHost::Stop)
}

fn send(
    service: &Arc<BrowserService>,
    command: lynx_browser_protocol::message::ToHost,
) -> Result<(), IpcError> {
    service
        .send(command)
        .map_err(|error| AppError::from(error).into())
}

#[tauri::command]
#[specta::specta]
pub async fn browser_state(
    service: State<'_, Arc<BrowserService>>,
) -> Result<BrowserState, IpcError> {
    Ok(service.state())
}

/// Shows the page in the window this is called from, over the rectangle the interface reserved
/// for it: logical pixels, relative to the top-left corner of the window's content. A page is
/// shown in one window at a time, so any other window loses it.
#[tauri::command]
#[specta::specta]
pub async fn browser_place(
    app: AppHandle,
    window: WebviewWindow,
    service: State<'_, Arc<BrowserService>>,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), IpcError> {
    #[cfg(any(target_os = "linux", windows))]
    {
        let service = Arc::clone(&service);
        let handle = app.clone();
        let label = window.label().to_owned();
        app.run_on_main_thread(move || {
            let rect = (x, y, width, height);
            if let Err(error) = super::platform_view::place(&handle, &service, &label, rect) {
                eprintln!("browser: {error}");
            }
        })
        .map_err(|error| IpcError::from(AppError::TaskFailed(error.to_string())))
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = (app, window, service, x, y, width, height);
        Err(AppError::from(BrowserError::Unsupported).into())
    }
}

/// Takes the page off the window this is called from, when its view is not shown or something
/// must be drawn over it.
#[tauri::command]
#[specta::specta]
pub async fn browser_hide(
    app: AppHandle,
    window: WebviewWindow,
    service: State<'_, Arc<BrowserService>>,
) -> Result<(), IpcError> {
    #[cfg(any(target_os = "linux", windows))]
    {
        let service = Arc::clone(&service);
        let label = window.label().to_owned();
        app.run_on_main_thread(move || super::platform_view::hide(&service, &label))
            .map_err(|error| IpcError::from(AppError::TaskFailed(error.to_string())))
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = (app, window, service);
        Ok(())
    }
}

/// Moves the page to a window of its own. The page keeps running: only where it is shown changes.
#[tauri::command]
#[specta::specta]
pub async fn browser_detach(
    app: AppHandle,
    service: State<'_, Arc<BrowserService>>,
) -> Result<(), IpcError> {
    if service.state().status != BrowserStatus::Running {
        return Err(AppError::from(BrowserError::NotRunning).into());
    }
    // The main window gives the page up first, so it never shows it twice.
    service.set_detached(true);
    detached_window::open(&app).map_err(|error| {
        service.set_detached(false);
        IpcError::from(AppError::from(error))
    })
}

/// Brings the page back to the main window and closes the browser window.
#[tauri::command]
#[specta::specta]
pub async fn browser_attach(
    app: AppHandle,
    service: State<'_, Arc<BrowserService>>,
) -> Result<(), IpcError> {
    service.set_detached(false);
    detached_window::close(&app);
    detached_window::show_main(&app);
    Ok(())
}

/// Stops the engine. The next open starts a new one.
#[tauri::command]
#[specta::specta]
pub async fn browser_close(
    app: AppHandle,
    service: State<'_, Arc<BrowserService>>,
) -> Result<(), IpcError> {
    service.set_detached(false);
    detached_window::close(&app);
    let service = Arc::clone(&service);
    run_blocking(move || {
        service.shutdown();
        Ok(())
    })
    .await
}
