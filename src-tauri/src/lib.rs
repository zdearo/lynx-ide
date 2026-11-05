mod agents;
mod app_info;
mod app_log;
mod bots;
mod browser;
mod changes;
mod config;
#[cfg(debug_assertions)]
mod dev_scope;
mod error;
mod files;
mod git;
mod ipc;
mod links;
mod orchestration;
mod persistence;
mod private_file;
mod process;
mod pull_request;
mod team;
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
use process::app_image_environment;
use terminal::terminal_events::TauriEventSink;
use terminal::terminal_manager::TerminalManager;
use usage::plan_usage_service::PlanUsageService;
use usage::{claude_usage, codex_usage};
use watching::watch_commands::WatchState;
use workspace::workspace_service::WorkspaceService;

/// `LYNX_HOME` wins, so tests and portable setups can keep everything in one folder.
#[cfg(not(debug_assertions))]
fn lynx_home(user_home: PathBuf) -> PathBuf {
    std::env::var_os("LYNX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| user_home.join(".lynx"))
}

/// A debug build keeps to its branch's own folder unless `LYNX_HOME` says otherwise.
#[cfg(debug_assertions)]
fn lynx_home(user_home: PathBuf) -> PathBuf {
    match std::env::var_os("LYNX_HOME") {
        Some(home) => PathBuf::from(home),
        None => dev_scope::dev_home(dev_scope_branch(), &user_home),
    }
}

/// The branch of this debug build, read once: every call must agree, or the data would split.
#[cfg(debug_assertions)]
fn dev_scope_branch() -> &'static str {
    static BRANCH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    BRANCH.get_or_init(|| {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        dev_scope::dev_branch(manifest.parent().unwrap_or(manifest))
    })
}

#[cfg(not(debug_assertions))]
fn app_database_file(app_data: PathBuf) -> PathBuf {
    app_data.join("app.db")
}

#[cfg(debug_assertions)]
fn app_database_file(app_data: PathBuf) -> PathBuf {
    dev_scope::dev_database(&app_data, dev_scope_branch())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // The harness runs this same binary for each hook event; it answers and exits without a window.
    let arguments: Vec<String> = std::env::args().collect();
    if let Some(code) = agents::hook_relay::run_if_requested(&arguments)
        .or_else(|| agents::tool_cli::run_if_requested(&arguments))
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
            // The log starts first, so what follows is recorded. Without it the app still runs.
            if let Err(error) = app_log::init(&lynx_home(user_home.clone())) {
                eprintln!("lynx: could not start the log: {error}");
            }
            #[cfg(debug_assertions)]
            log::info!(
                "debug build on branch \"{}\": LYNX_HOME {}",
                dev_scope_branch(),
                lynx_home(user_home.clone()).display()
            );
            let executable = app_image_environment::launch_executable(std::env::current_exe().ok());
            // A debug build keeps its agents' settings in its own folder, so it never rewrites the installed
            // app's hooks or MCP entry in the person's home.
            #[cfg(debug_assertions)]
            let agents_home = lynx_home(user_home.clone());
            #[cfg(not(debug_assertions))]
            let agents_home = user_home.clone();
            let hook_targets = HookTargets::from_environment(&agents_home, executable.as_deref());
            // Agents turned on before the browser tools existed get them too; this waits on the
            // agents' CLIs, so it stays off the startup path.
            let pending_tools = hook_targets.clone();
            let _ = std::thread::Builder::new()
                .name("browser-tools-install".to_owned())
                .spawn(move || pending_tools.refresh_agent_tools());
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

            // The ChatGPT sign-in keeps its credentials beside the config, never inside it.
            let chatgpt_login = Arc::new(bots::chatgpt_login::ChatGptLogin::new(
                app.handle().clone(),
                lynx_home(app.path().home_dir()?)
                    .join("bots")
                    .join("chatgpt.json"),
            ));
            app.manage(chatgpt_login);

            // Every bot has its own folder under Lynx's data. Each bot's Claude runs from there, and
            // the user's home is kept out of its memory.
            let bots_dir = lynx_home(app.path().home_dir()?).join("bots");
            app.manage(Arc::new(bots::bot_folder::BotsRoot(bots_dir.clone())));
            let claude_cli = Arc::new(bots::claude_cli::ClaudeCli::new(
                bots_dir.clone(),
                app.path().home_dir()?,
            ));
            app.manage(Arc::clone(&claude_cli));
            let bot_chat = Arc::new(bots::bot_chat::BotChat::new(
                bots_dir.clone(),
                Arc::clone(&claude_cli),
            ));
            // Read per turn, so the switch in the settings applies without a restart.
            let capture_config = Arc::clone(&config);
            bot_chat.attach_capture(Arc::new(move || {
                capture_config.config().bots.memory.capture
            }));
            app.manage(bot_chat);
            let channels_dir = lynx_home(app.path().home_dir()?).join("channels");
            let channel_chat = Arc::new(bots::channel_chat::ChannelChat::new(
                bots_dir.clone(),
                channels_dir,
                claude_cli,
            ));
            // Jev picks who answers when the user turned it on and saved a key; both are read per message.
            let decider_config = Arc::clone(&config);
            channel_chat.attach_decider(Arc::new(bots::jev_decider::JevDecider::new(
                bots_dir.join("jev.key"),
                Arc::new(move || decider_config.config().bots.decider.enabled),
            )));
            app.manage(channel_chat);

            // AppDatabase::open creates the folder the file sits in.
            let database_file = app_database_file(app.path().app_data_dir()?);
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
            // A bot's Claude reaches the app through the hub, started from this executable. The registered
            // repositories are what it may ask to read.
            let registered: Arc<dyn bots::bot_access::RegisteredRepos> = Arc::new(
                workspace::registered_repos::RegisteredWorkspaces(app.handle().clone()),
            );
            app.manage(Arc::clone(&registered));
            if let Some(executable) =
                app_image_environment::launch_executable(std::env::current_exe().ok())
            {
                app.state::<Arc<bots::claude_cli::ClaudeCli>>().attach(
                    bots::bot_bridge::BotBridge::new(
                        hook_hub.clone(),
                        executable,
                        Arc::clone(&registered),
                    ),
                );
            }
            let terminal_database = Arc::new(AppDatabase::open(&database_file)?);
            // The team has its own connection to the same file.
            // A bot joins a team only while its folder is under the bots folder. One check is built here:
            // the service keeps a copy and the startup reconcile below reads the same bots folder.
            let team_database = Arc::new(AppDatabase::open(&database_file)?);
            let team_bots = bots_dir.clone();
            let bot_exists =
                move |bot_id: &str| bots::bot_folder::folder_of(&team_bots, bot_id).is_ok();
            let team = Arc::new(team::team_service::TeamService::new(
                team_database,
                Box::new(bot_exists.clone()),
            ));
            app.manage(Arc::clone(&team));
            // Orchestration works on the team's members. It reads terminals, worktrees and settings from the
            // managed services when a call needs them.
            let orchestration = Arc::new(orchestration::service::Orchestration::new(
                Arc::clone(&team),
                Arc::new(orchestration::host::AppWorkers::new(
                    app.handle().clone(),
                    Arc::clone(&config),
                )),
            ));
            app.manage(Arc::clone(&orchestration));
            // A bot deleted before its team lines went with it may have left members behind. Only a folder that is
            // really gone counts: an unreadable bot.json must not clear a bot's team. Best effort: a failure is
            // logged and the app still starts.
            // The check answers "does the bot still exist": the team keeps every bot whose folder is there.
            let bot_folder_exists = |bot_id: &str| bots_dir.join(bot_id).is_dir();
            match team.remove_missing_bots(bot_folder_exists) {
                Ok(0) => {}
                Ok(removed) => {
                    log::info!("removed {removed} team members of bots that no longer exist")
                }
                Err(error) => log::error!("could not clear the team of deleted bots: {error}"),
            }
            let announce = bots::bots_commands::announce_message(app.handle().clone());
            let bot_chat = Arc::clone(&*app.state::<Arc<bots::bot_chat::BotChat>>());
            bot_chat.attach_announce(Arc::clone(&announce));
            let bot_chat_for_commands = Arc::clone(&bot_chat);
            let bot_processes = Arc::new(bots::bot_processes::BotProcesses::new(
                Arc::new(bots::app_command_host::AppCommandHost::new(
                    app.handle().clone(),
                )),
                lynx_home(app.path().home_dir()?).join("bots"),
            ));
            app.manage(Arc::clone(&bot_processes));
            // A command a bot left running that ends by itself wakes the bot, as a finished worker does.
            let waking = Arc::clone(&bot_chat_for_commands);
            bot_processes.on_ended(Arc::new(move |bot: &str, label: &str, note: &str| {
                waking.wake(bot, label, note);
                bots::bot_chat::BotChat::kick(
                    Arc::clone(&waking),
                    bot.to_owned(),
                    label.to_owned(),
                );
            }));
            let bot_tools = Arc::new(bots::bot_tools::BotTools::new(
                bot_chat,
                Arc::clone(&registered),
                announce,
                bot_processes,
            ));
            hook_hub.set_tool_handler(Arc::new(
                orchestration::router::ToolRouter::new(
                    Arc::new(browser::AgentToolBridge::new(
                        app.handle().clone(),
                        browser_service,
                        Arc::clone(&config),
                        owners.clone(),
                    )),
                    orchestration,
                )
                .with_bots(bot_tools),
            ));

            let output_channels = Arc::new(terminal::output_channels::OutputChannels::default());
            app.manage(Arc::clone(&output_channels));
            let shell_config = Arc::clone(&config);
            let daemon_config = Arc::clone(&config);
            let daemon = terminal::daemon_host::DaemonHost::new(
                daemon_dir(&lynx_home(app.path().home_dir()?)),
                executable.clone(),
                Arc::new(move || daemon_config.config().terminal.pty_daemon),
            );
            let terminals = TerminalManager::new(
                Arc::new(move || {
                    terminal::Shell::from_config(shell_config.config().terminal.shell.as_deref())
                }),
                Arc::new(TauriEventSink::new(
                    app.handle().clone(),
                    Some(browser_for_events),
                    output_channels,
                )),
                hook_hub,
                owners,
                Some(daemon),
                Some(terminal_database),
            );
            terminals.restore();
            app.manage(Arc::new(orchestration::agent_wake::AgentWakes::new(
                terminals.clone(),
            )));
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
