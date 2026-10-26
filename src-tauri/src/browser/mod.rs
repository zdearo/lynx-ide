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
mod page_owner;
mod page_snapshot;
mod page_thumbnail;
mod web_address;

use std::sync::Arc;

use lynx_browser_protocol::message::PageId;
use tauri::{AppHandle, Manager};
use tauri_specta::Event;

pub use agent_browser_bridge::{AgentToolBridge, BrowserAgentActivity, BrowserAgentReleased};
pub use agent_permissions::AgentBrowserAccess;
pub use browser_error::BrowserError;
pub use browser_service::{BrowserEvents, BrowserService, FrameSink};
pub use browser_state::{
    BrowserPageInfo, BrowserPagesChanged, BrowserState, BrowserStateChanged,
    BrowserThumbnailChanged,
};
pub use engine_commands::BrowserEngineProgress;
pub use page_owner::{PageOwner, SessionOwners};

/// Keeps the tabs and their windows in step: closing a tab's window brings the tab back to the main
/// view, and closing the main window closes every tab window too.
pub fn on_window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    if !matches!(event, tauri::WindowEvent::Destroyed) {
        return;
    }
    let app = window.app_handle();
    let label = window.label().to_owned();
    if detached_window::is_main_window(&label) {
        detached_window::close_all(app);
    } else if let Some(page) = detached_window::page_of_label(&label) {
        let service = Arc::clone(&*app.state::<Arc<BrowserService>>());
        // The window's view goes first: once the tab is attached it is no longer in a window of
        // its own, and releasing the view would then tell the engine nobody is looking at it.
        #[cfg(any(target_os = "linux", windows))]
        {
            let _ = app.run_on_main_thread(move || {
                platform_view::forget(&service, &label);
                let _ = service.attach_page(page);
            });
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        {
            let _ = (&label, service.attach_page(page));
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

    fn pages_changed(&self, pages: &[BrowserPageInfo]) {
        let _ = BrowserPagesChanged {
            pages: pages.to_vec(),
        }
        .emit(&self.app);
    }

    fn thumbnail_changed(&self, page: PageId, png: &str) {
        let _ = BrowserThumbnailChanged {
            page,
            png: png.to_owned(),
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

#[cfg(test)]
mod browser_perf_tests;
