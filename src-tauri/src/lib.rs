mod agents;
mod app_image_environment;
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
mod updates;
mod usage;
mod watching;
mod workspace;

use lynx_pty_daemon::server::daemon_dir;
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
use usage::plan_usage_service::PlanUsageService;
use usage::{claude_usage, codex_usage};
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
        .or_else(|| terminal::daemon_host::run_if_requested(&arguments))
        .or_else(|| {
            let folder = daemon_dir(&lynx_home(std::env::home_dir()?));
            terminal::daemon_host::start_outside_if_requested(
                &arguments,
                &folder,
                std::env::current_exe().ok(),
            )
        })
    {
        std::process::exit(code);
    }

    let builder = ipc::builder();

    let application = tauri::Builder::default();
    // One Lynx at a time: a second one would take the hub's address file from the first. The
    // plugin goes first, as it must. A debug build is left out so `tauri dev` runs beside the
    // installed app.
    #[cfg(all(desktop, not(debug_assertions)))]
    let application = application.plugin(tauri_plugin_single_instance::init(|app, _, _| {
        use tauri::Manager;
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
        }
    }));
    application
        .on_window_event(browser::on_window_event)
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);

            // Desktop only. Checks run from Rust, so the webview gets no updater permission.
            #[cfg(desktop)]
            app.handle()
                .plugin(tauri_plugin_updater::Builder::new().build())?;

            let user_home = app.path().home_dir()?;
            let executable = app_image_environment::launch_executable(std::env::current_exe().ok());
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

            let updates = Arc::new(updates::update_service::UpdateService::new(
                app.handle().clone(),
            ));
            Arc::clone(&updates).start_schedule(Arc::clone(&config));
            app.manage(updates);

            let database_file = app.path().app_data_dir()?.join("app.db");
            let worktrees_config = Arc::clone(&config);
            app.manage(WorkspaceService::new(
                AppDatabase::open(&database_file)?,
                Arc::new(move || worktrees_config.worktrees_directory()),
            ));

            app.manage(WatchState::default());
            app.manage(PlanUsageService::new(
                codex_usage::installed(),
                claude_usage::installed(),
            ));

            let browser_home = lynx_home(app.path().home_dir()?);
            let browser_service = Arc::new(browser::BrowserService::new(
                browser_home,
                Arc::new(browser::TauriBrowserEvents::new(app.handle().clone())),
                browser::frame_sink(app.handle().clone()),
            ));
            let browser_for_events = Arc::clone(&browser_service);
            app.manage(Arc::clone(&browser_service));

            // The manager records each session's owner; the browser tools read it.
            let owners = browser::SessionOwners::default();

            // Agents reach the browser through the same loopback listener their hooks use.
            let hook_hub = HookHub::start();
            // A debug build keeps to its own variables unless it has a home of its own, so a
            // `tauri dev` next to the installed app does not take the file over from it.
            if !cfg!(debug_assertions) || std::env::var_os("LYNX_HOME").is_some() {
                // Without the file, agents fall back to the variables they started with.
                let _ =
                    hook_hub.publish_address(&lynx_home(app.path().home_dir()?).join("hub.json"));
            }
            hook_hub.set_tool_handler(Arc::new(browser::AgentToolBridge::new(
                app.handle().clone(),
                browser_service,
                Arc::clone(&config),
                owners.clone(),
            )));

            let shell_config = Arc::clone(&config);
            let daemon_config = Arc::clone(&config);
            let daemon = terminal::daemon_host::DaemonHost::new(
                daemon_dir(&lynx_home(app.path().home_dir()?)),
                executable.clone(),
                Arc::new(move || daemon_config.config().terminal.pty_daemon),
            );
            let terminal_database = Arc::new(AppDatabase::open(&database_file)?);
            let terminals = TerminalManager::new(
                Arc::new(move || {
                    terminal::Shell::from_config(shell_config.config().terminal.shell.as_deref())
                }),
                Arc::new(TauriEventSink::new(
                    app.handle().clone(),
                    Some(browser_for_events),
                )),
                hook_hub,
                owners,
                Some(daemon),
                Some(terminal_database),
            );
            terminals.restore();
            app.manage(terminals);
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // Local shells end with the app. The daemon's sessions end too, unless the user kept them.
            if let RunEvent::Exit = event {
                app.state::<TerminalManager>().quit();
                app.state::<Arc<browser::BrowserService>>().shutdown();
            }
        });
}
