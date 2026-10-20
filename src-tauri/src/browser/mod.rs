//! The browser. The engine (CEF) runs in its own process, `lynx-browser-host`; this module starts
//! it, talks to it through `lynx-browser-protocol`, shows its pages in a native view, and tells
//! the interface what the page is doing. See `scripts/browser-engine.mjs` for installing it.

mod agent_browser;
mod agent_browser_bridge;
mod agent_icon;
mod agent_permissions;
mod agent_pointer;
pub mod agent_tool_definitions;
pub mod browser_commands;
mod browser_error;
mod browser_service;
mod browser_state;
#[cfg(any(target_os = "linux", windows))]
mod click_counter;
mod detached_window;
pub mod engine_commands;
mod engine_files;
mod engine_install_error;
mod engine_installer;
mod engine_location;
mod engine_manifest;
#[cfg(target_os = "linux")]
mod key_codes;
mod key_names;
#[cfg(target_os = "linux")]
mod native_view;
#[cfg(windows)]
mod windows_view;

/// The platform's native view of the page, which draws the engine's frames and takes the input.
#[cfg(target_os = "linux")]
use native_view as platform_view;
#[cfg(windows)]
use windows_view as platform_view;
#[cfg(any(windows, test))]
mod overlay_renderer;
mod page_log;
mod page_snapshot;
mod web_address;

use std::sync::Arc;

use tauri::{AppHandle, Manager};
use tauri_specta::Event;

pub use agent_browser_bridge::{AgentToolBridge, BrowserAgentActivity, BrowserAgentReleased};
pub use agent_permissions::AgentBrowserAccess;
pub use agent_pointer::AgentPointer;
pub use browser_error::BrowserError;
pub use browser_service::{BrowserEvents, BrowserService, FrameSink};
pub use browser_state::{BrowserState, BrowserStateChanged};
pub use engine_commands::BrowserEngineProgress;

/// Keeps the page and its windows in step: closing the browser window moves the page back to the
/// main window, and closing the main window closes the browser window too.
pub fn on_window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    if !matches!(event, tauri::WindowEvent::Destroyed) {
        return;
    }
    let app = window.app_handle();
    if detached_window::is_main_window(window.label()) {
        detached_window::close(app);
    } else if detached_window::is_browser_window(window.label()) {
        let service = Arc::clone(&*app.state::<Arc<BrowserService>>());
        service.set_detached(false);
        #[cfg(any(target_os = "linux", windows))]
        {
            let _ = app.run_on_main_thread(move || {
                platform_view::forget(&service, detached_window::LABEL);
            });
        }
        detached_window::show_main(app);
    }
}

/// Tells the interface through the Tauri event system.
pub struct TauriBrowserEvents {
    app: AppHandle,
}

impl TauriBrowserEvents {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl BrowserEvents for TauriBrowserEvents {
    fn state_changed(&self, state: &BrowserState) {
        // A failed emit means the window is closing; there is no one left to tell.
        let _ = BrowserStateChanged {
            state: state.clone(),
        }
        .emit(&self.app);
    }

    fn agent_released(&self) {
        let _ = BrowserAgentReleased {}.emit(&self.app);
    }
}

/// Where this platform shows frames: its native view, or nowhere on a platform without one yet.
pub fn frame_sink(app: AppHandle) -> Arc<dyn FrameSink> {
    #[cfg(any(target_os = "linux", windows))]
    {
        Arc::new(platform_view::NativeView::new(app))
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        struct Nowhere;
        impl FrameSink for Nowhere {
            fn frame(&self, _frame: browser_service::Frame) {}
        }
        let _ = app;
        Arc::new(Nowhere)
    }
}
