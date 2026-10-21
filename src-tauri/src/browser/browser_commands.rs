use std::sync::Arc;

use lynx_browser_protocol::message::PageCommand;
use tauri::{AppHandle, Manager, State, WebviewWindow};

use crate::blocking_task::run_blocking;
use crate::error::{AppError, IpcError};

use super::browser_error::BrowserError;
use super::browser_service::BrowserService;
use super::browser_state::{BrowserPageInfo, BrowserState, BrowserThumbnail};
use super::detached_window;
use super::page_owner::PageOwner;

/// Starts the browser engine if needed and optionally opens an address in the shown page. The
/// engine's start-up blocks for a moment, so this runs off the async runtime.
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
    send(&service, PageCommand::Back)
}

#[tauri::command]
#[specta::specta]
pub async fn browser_forward(service: State<'_, Arc<BrowserService>>) -> Result<(), IpcError> {
    send(&service, PageCommand::Forward)
}

#[tauri::command]
#[specta::specta]
pub async fn browser_reload(
    service: State<'_, Arc<BrowserService>>,
    ignore_cache: bool,
) -> Result<(), IpcError> {
    send(&service, PageCommand::Reload { ignore_cache })
}

#[tauri::command]
#[specta::specta]
pub async fn browser_stop(service: State<'_, Arc<BrowserService>>) -> Result<(), IpcError> {
    send(&service, PageCommand::Stop)
}

fn send(service: &Arc<BrowserService>, command: PageCommand) -> Result<(), IpcError> {
    service
        .send_shown(command)
        .map_err(|error| AppError::from(error).into())
}

#[tauri::command]
#[specta::specta]
pub async fn browser_state(
    service: State<'_, Arc<BrowserService>>,
) -> Result<BrowserState, IpcError> {
    Ok(service.state())
}

/// The state of one page, for the window that shows it.
#[tauri::command]
#[specta::specta]
pub async fn browser_page_state(
    service: State<'_, Arc<BrowserService>>,
    page: u32,
) -> Result<BrowserState, IpcError> {
    Ok(service.state_of(page))
}

#[tauri::command]
#[specta::specta]
pub async fn browser_pages(
    service: State<'_, Arc<BrowserService>>,
) -> Result<Vec<BrowserPageInfo>, IpcError> {
    Ok(service.pages())
}

#[tauri::command]
#[specta::specta]
pub async fn browser_thumbnails(
    service: State<'_, Arc<BrowserService>>,
) -> Result<Vec<BrowserThumbnail>, IpcError> {
    Ok(service.thumbnails())
}

/// Opens a page for a project's worktree and shows it. The engine makes the page and may block
/// for a moment, so this runs off the async runtime.
#[tauri::command]
#[specta::specta]
pub async fn browser_page_new(
    service: State<'_, Arc<BrowserService>>,
    worktree_path: String,
    url: Option<String>,
) -> Result<BrowserPageInfo, IpcError> {
    if cfg!(not(any(target_os = "linux", windows))) {
        return Err(AppError::from(BrowserError::Unsupported).into());
    }
    let service = Arc::clone(&service);
    run_blocking(move || {
        Ok(service.new_page(PageOwner::Workspace { worktree_path }, url.as_deref())?)
    })
    .await
}

/// Shows a page to the person. A page in its own window is not taken from it: that window is
/// brought forward instead.
#[tauri::command]
#[specta::specta]
pub async fn browser_page_show(
    app: AppHandle,
    service: State<'_, Arc<BrowserService>>,
    page: u32,
) -> Result<BrowserState, IpcError> {
    let state = service
        .show_page(page)
        .map_err(|error| IpcError::from(AppError::from(error)))?;
    if service.is_detached(page) {
        detached_window::focus(&app, page);
    }
    Ok(state)
}

/// Closes a page, and its window if it has one. A page that is already gone counts as closed.
#[tauri::command]
#[specta::specta]
pub async fn browser_page_close(
    app: AppHandle,
    service: State<'_, Arc<BrowserService>>,
    page: u32,
) -> Result<(), IpcError> {
    detached_window::close(&app, page);
    service
        .close_page(page)
        .map_err(|error| AppError::from(error).into())
}

/// Moves a page into its own window. The main view stops showing it; the page keeps running.
#[tauri::command]
#[specta::specta]
pub async fn browser_page_detach(
    app: AppHandle,
    service: State<'_, Arc<BrowserService>>,
    page: u32,
) -> Result<(), IpcError> {
    service
        .detach_page(page)
        .map_err(|error| IpcError::from(AppError::from(error)))?;
    detached_window::open(&app, page).map_err(|error| {
        let _ = service.attach_page(page);
        IpcError::from(AppError::from(error))
    })
}

/// Brings a page back from its own window into the main view, and closes that window.
#[tauri::command]
#[specta::specta]
pub async fn browser_page_attach(
    app: AppHandle,
    service: State<'_, Arc<BrowserService>>,
    page: u32,
) -> Result<(), IpcError> {
    service
        .attach_page(page)
        .map_err(|error| IpcError::from(AppError::from(error)))?;
    detached_window::close(&app, page);
    detached_window::show_main(&app);
    Ok(())
}

/// Shows the page in the window this is called from, over the rectangle the interface reserved
/// for it: logical pixels, relative to the top-left corner of the window's content.
#[tauri::command]
#[specta::specta]
pub async fn browser_place(
    app: AppHandle,
    window: WebviewWindow,
    page: u32,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), IpcError> {
    #[cfg(any(target_os = "linux", windows))]
    {
        let service = Arc::clone(&*app.state::<Arc<BrowserService>>());
        let handle = app.clone();
        let label = window.label().to_owned();
        app.run_on_main_thread(move || {
            let rect = (x, y, width, height);
            if let Err(error) = super::platform_view::place(&handle, &service, &label, page, rect) {
                eprintln!("browser: {error}");
            }
        })
        .map_err(|error| IpcError::from(AppError::TaskFailed(error.to_string())))
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = (app, window, page, x, y, width, height);
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

/// Stops the engine. The next open starts a new one, and every tab window closes with it.
#[tauri::command]
#[specta::specta]
pub async fn browser_close(
    app: AppHandle,
    service: State<'_, Arc<BrowserService>>,
) -> Result<(), IpcError> {
    detached_window::close_all(&app);
    let service = Arc::clone(&service);
    run_blocking(move || {
        service.shutdown();
        Ok(())
    })
    .await
}
