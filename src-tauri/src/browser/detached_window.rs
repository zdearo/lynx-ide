//! The windows of tabs taken out of the main view. A tab does not live in a window: the engine
//! renders it in its own process, and a native view in some window shows it. Detaching a tab opens
//! a window for it and moves its view there; closing the window brings the tab back. Each tab has
//! its own window, and nothing reloads.

use lynx_browser_protocol::message::PageId;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use super::browser_error::BrowserError;

const MAIN: &str = "main";
/// The label prefix of a tab's window; the page id follows it.
const PREFIX: &str = "browser-page-";

/// The label of the window that shows `page`.
pub fn label_of(page: PageId) -> String {
    format!("{PREFIX}{page}")
}

/// The page a window's label names, if it is a tab's window.
pub fn page_of_label(label: &str) -> Option<PageId> {
    label.strip_prefix(PREFIX)?.parse().ok()
}

/// Opens the window of `page`, or brings it forward if it is already open.
pub fn open(app: &AppHandle, page: PageId) -> Result<(), BrowserError> {
    let label = label_of(page);
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.set_focus();
        return Ok(());
    }
    // The interface reads `window=browser` and the page to show the tab alone, not the whole app.
    WebviewWindowBuilder::new(
        app,
        &label,
        WebviewUrl::App(format!("index.html?window=browser&page={page}").into()),
    )
    .title("Lynx Browser")
    .inner_size(1100.0, 760.0)
    .min_inner_size(480.0, 320.0)
    .build()
    .map_err(|error| BrowserError::Window(error.to_string()))?;
    Ok(())
}

/// Brings the window of `page` forward, if it is open.
pub fn focus(app: &AppHandle, page: PageId) {
    if let Some(window) = app.get_webview_window(&label_of(page)) {
        let _ = window.set_focus();
    }
}

/// Closes the window of `page`, if it is open.
pub fn close(app: &AppHandle, page: PageId) {
    if let Some(window) = app.get_webview_window(&label_of(page)) {
        let _ = window.destroy();
    }
}

/// Closes every tab window, when the engine stops or the main window closes.
pub fn close_all(app: &AppHandle) {
    for (label, window) in app.webview_windows() {
        if label.starts_with(PREFIX) {
            let _ = window.destroy();
        }
    }
}

/// Brings the main window forward, where a tab returns to.
pub fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN) {
        let _ = window.set_focus();
    }
}

pub fn is_main_window(label: &str) -> bool {
    label == MAIN
}
