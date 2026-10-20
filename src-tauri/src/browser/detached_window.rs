//! The browser's own window. The page does not live in a window: the engine renders it in its
//! own process, and a native view in some window shows it. Detaching moves that view to this
//! window; closing the window moves it back. Nothing reloads.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use super::browser_error::BrowserError;

pub const LABEL: &str = "browser";
const MAIN: &str = "main";

/// Opens the window, or brings it forward if it is already open.
pub fn open(app: &AppHandle) -> Result<(), BrowserError> {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.set_focus();
        return Ok(());
    }
    // The interface reads `window=browser` to show the browser alone, not the whole app.
    WebviewWindowBuilder::new(
        app,
        LABEL,
        WebviewUrl::App("index.html?window=browser".into()),
    )
    .title("Lynx Browser")
    .inner_size(1100.0, 760.0)
    .min_inner_size(480.0, 320.0)
    .build()
    .map_err(|error| BrowserError::Window(error.to_string()))?;
    Ok(())
}

/// Closes the window if it is open.
pub fn close(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.destroy();
    }
}

/// Brings the main window forward, where the page returns to.
pub fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN) {
        let _ = window.set_focus();
    }
}

pub fn is_browser_window(label: &str) -> bool {
    label == LABEL
}

pub fn is_main_window(label: &str) -> bool {
    label == MAIN
}
