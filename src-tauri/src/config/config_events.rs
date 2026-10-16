use std::sync::Arc;
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;
use tauri_specta::Event;

use super::config_store::{ConfigSnapshot, ConfigStore};

/// The config file changed outside the app (an editor, a dotfiles sync, a script).
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct ConfigChanged {
    pub snapshot: ConfigSnapshot,
}

const POLL_INTERVAL: Duration = Duration::from_secs(1);

/// Polls the file's size and modification time once a second. It is one `stat` call,
/// cheaper and more portable than a native file watcher for a single small file.
pub fn start_config_watcher(store: Arc<ConfigStore>, app: AppHandle) {
    // Without the watcher the app still works; edits just need a restart to show up.
    let _ = thread::Builder::new()
        .name("config-watcher".to_owned())
        .spawn(move || {
            loop {
                thread::sleep(POLL_INTERVAL);
                if let Some(snapshot) = store.reload_if_changed() {
                    let _ = ConfigChanged { snapshot }.emit(&app);
                }
            }
        });
}
