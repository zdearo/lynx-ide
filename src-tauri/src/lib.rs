mod app_info;
mod blocking_task;
mod error;
mod git;
mod ipc;
mod persistence;
mod workspace;

use tauri::Manager;

use persistence::AppDatabase;
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
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
