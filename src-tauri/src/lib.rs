mod app_info;
mod blocking_task;
mod config;
mod error;
mod files;
mod git;
mod ipc;
mod persistence;
mod terminal;
mod workspace;

use std::path::PathBuf;
use std::sync::Arc;

use tauri::{Manager, RunEvent};

use config::ConfigStore;
use config::config_events::start_config_watcher;
use persistence::AppDatabase;
use terminal::terminal_events::TauriEventSink;
use terminal::terminal_manager::TerminalManager;
use workspace::workspace_service::WorkspaceService;

/// `LYNX_HOME` wins, so tests and portable setups can keep everything in one folder.
fn lynx_home(user_home: PathBuf) -> PathBuf {
    std::env::var_os("LYNX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| user_home.join(".lynx"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = ipc::builder();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);

            let user_home = app.path().home_dir()?;
            let config = Arc::new(ConfigStore::open(
                lynx_home(user_home.clone()).join("config.json"),
                user_home,
            ));
            start_config_watcher(Arc::clone(&config), app.handle().clone());
            app.manage(Arc::clone(&config));

            let database_file = app.path().app_data_dir()?.join("app.db");
            let worktrees_config = Arc::clone(&config);
            app.manage(WorkspaceService::new(
                AppDatabase::open(&database_file)?,
                Arc::new(move || worktrees_config.worktrees_directory()),
            ));

            let shell_config = Arc::clone(&config);
            app.manage(TerminalManager::new(
                Arc::new(move || {
                    terminal::Shell::from_config(shell_config.config().terminal.shell.as_deref())
                }),
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
