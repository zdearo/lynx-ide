mod app_info;
mod blocking_task;
mod error;
mod git;
mod ipc;
mod persistence;
mod terminal;
mod workspace;

use std::sync::Arc;

use tauri::{Manager, RunEvent};

use persistence::AppDatabase;
use terminal::terminal_events::TauriEventSink;
use terminal::terminal_manager::TerminalManager;
use workspace::workspace_service::WorkspaceService;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = ipc::builder();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);

            let database_file = app.path().app_data_dir()?.join("app.db");
            app.manage(WorkspaceService::new(AppDatabase::open(&database_file)?));
            app.manage(TerminalManager::new(
                terminal::Shell::system_default(),
                Arc::new(TauriEventSink::new(app.handle().clone())),
            ));
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // Without this, shells and agents would outlive the window.
            if let RunEvent::Exit = event {
                app.state::<TerminalManager>().close_all();
            }
        });
}
