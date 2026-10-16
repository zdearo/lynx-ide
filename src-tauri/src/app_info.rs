use serde::Serialize;
use specta::Type;

#[derive(Serialize, Type)]
pub struct AppInfo {
    name: &'static str,
    version: &'static str,
}

#[tauri::command]
#[specta::specta]
pub fn app_info() -> AppInfo {
    AppInfo {
        name: env!("CARGO_PKG_NAME"),
        version: env!("CARGO_PKG_VERSION"),
    }
}
