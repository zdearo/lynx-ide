mod agents;
mod app_info;
mod blocking_task;
mod bounded_command;
mod browser;
mod changes;
mod config;
mod error;
mod files;
mod git;
mod ipc;
mod persistence;
mod pull_request;
mod terminal;
mod watching;
mod workspace;

use std::path::PathBuf;
use std::sync::Arc;

use tauri::{Manager, RunEvent};

use agents::hook_hub::HookHub;
use agents::hook_targets::HookTargets;
use config::ConfigStore;
use config::config_events::start_config_watcher;
use persistence::AppDatabase;
use terminal::terminal_events::TauriEventSink;
use terminal::terminal_manager::TerminalManager;
use watching::watch_commands::WatchState;
use workspace::workspace_service::WorkspaceService;

/// `LYNX_HOME` wins, so tests and portable setups can keep everything in one folder.
fn lynx_home(user_home: PathBuf) -> PathBuf {
    std::env::var_os("LYNX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| user_home.join(".lynx"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // The harness runs this same binary for each hook event; it answers and exits without a window.
    let arguments: Vec<String> = std::env::args().collect();
    if let Some(code) = agents::hook_relay::run_if_requested(&arguments)
        .or_else(|| agents::mcp_server::run_if_requested(&arguments))
    {
        std::process::exit(code);
    }

    let builder = ipc::builder();

    tauri::Builder::default()
        .on_window_event(browser::on_window_event)
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);

            let user_home = app.path().home_dir()?;
            let executable = std::env::current_exe().ok();
            let hook_targets = HookTargets::from_environment(&user_home, executable.as_deref());
            // Agents turned on before the browser tools existed get them too; this waits on the
            // agents' CLIs, so it stays off the startup path.
            let pending_tools = hook_targets.clone();
            let _ = std::thread::Builder::new()
                .name("browser-tools-install".to_owned())
                .spawn(move || pending_tools.add_missing_browser_tools());
            app.manage(hook_targets);

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

            app.manage(WatchState::default());

            let browser_home = lynx_home(app.path().home_dir()?);
            let browser_service = Arc::new(browser::BrowserService::new(
                browser_home,
                Arc::new(browser::TauriBrowserEvents::new(app.handle().clone())),
                browser::frame_sink(app.handle().clone()),
            ));
            let browser_pointer = browser_service.pointer();
            app.manage(Arc::clone(&browser_service));

            // Agents reach the browser through the same loopback listener their hooks use.
            let hook_hub = HookHub::start();
            hook_hub.set_tool_handler(Arc::new(browser::AgentToolBridge::new(
                app.handle().clone(),
                browser_service,
                Arc::clone(&config),
            )));

            let shell_config = Arc::clone(&config);
            app.manage(TerminalManager::new(
                Arc::new(move || {
                    terminal::Shell::from_config(shell_config.config().terminal.shell.as_deref())
                }),
                Arc::new(TauriEventSink::new(
                    app.handle().clone(),
                    Some(browser_pointer),
                )),
                hook_hub,
            ));
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // Without this, shells and agents would outlive the window.
            if let RunEvent::Exit = event {
                app.state::<TerminalManager>().close_all();
                app.state::<Arc<browser::BrowserService>>().shutdown();
            }
        });
}
