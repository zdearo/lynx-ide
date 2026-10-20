use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use specta::Type;

use crate::agents::harness::{HarnessId, harness_name, supported};

use super::app_config::{
    AppConfig, BrowserConfig, ConfigIssue, DEFAULT_FONT_SIZE, DEFAULT_SCROLLBACK, GitConfig,
    LaunchMode, NotificationsConfig, TerminalConfig, expand_home, parse_config,
};

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("could not access the config file {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("the config file is not valid JSON; fix it by hand before changing settings here: {0}")]
    BrokenFile(String),
    #[error("{}", describe(.0))]
    InvalidValues(Vec<ConfigIssue>),
}

fn describe(issues: &[ConfigIssue]) -> String {
    issues
        .iter()
        .map(|issue| format!("{}: {}", issue.key, issue.message))
        .collect::<Vec<_>>()
        .join("; ")
}

/// What the interface sees: the effective settings plus everything it should warn about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConfigSnapshot {
    pub config: AppConfig,
    pub issues: Vec<ConfigIssue>,
    pub path: String,
    pub file_exists: bool,
    /// The file exists but is not valid JSON, so settings cannot be saved over it.
    pub file_broken: bool,
}

/// Size and modification time: enough to notice an edit made by another program.
type Fingerprint = Option<(SystemTime, u64)>;

struct Loaded {
    snapshot: ConfigSnapshot,
    fingerprint: Fingerprint,
}

/// `~/.lynx/config.json`. It is the user's file: it can be edited by hand, so every
/// read tolerates mistakes and every write keeps whatever it does not understand.
pub struct ConfigStore {
    path: PathBuf,
    home: PathBuf,
    loaded: Mutex<Loaded>,
}

impl ConfigStore {
    pub fn open(path: PathBuf, home: PathBuf) -> Self {
        let loaded = load(&path, &home);
        Self {
            path,
            home,
            loaded: Mutex::new(loaded),
        }
    }

    pub fn snapshot(&self) -> ConfigSnapshot {
        self.lock().snapshot.clone()
    }

    pub fn config(&self) -> AppConfig {
        self.lock().snapshot.config.clone()
    }

    /// The configured worktrees folder with `~` expanded, if any.
    pub fn worktrees_directory(&self) -> Option<PathBuf> {
        let config = self.config();
        config
            .worktrees
            .directory
            .as_deref()
            .map(|directory| expand_home(directory, &self.home))
    }

    /// Re-reads the file when something else changed it. Returns the new snapshot only then.
    pub fn reload_if_changed(&self) -> Option<ConfigSnapshot> {
        let mut loaded = self.lock();
        if fingerprint(&self.path) == loaded.fingerprint {
            return None;
        }
        *loaded = load(&self.path, &self.home);
        Some(loaded.snapshot.clone())
    }

    /// Saves `config`. Only values that differ from the defaults are written, and
    /// anything else already in the file (other keys, key order) is left as it was.
    pub fn update(&self, config: AppConfig) -> Result<ConfigSnapshot, ConfigError> {
        let config = config.normalized();
        let issues = config.validate(&self.home);
        if !issues.is_empty() {
            return Err(ConfigError::InvalidValues(issues));
        }

        let mut root = match fs::read_to_string(&self.path) {
            // A file with only whitespace is an empty config, like on load.
            Ok(text) if text.trim().is_empty() => Map::new(),
            Ok(text) => match serde_json::from_str::<Value>(&text) {
                Ok(Value::Object(map)) => map,
                Ok(_) => {
                    return Err(ConfigError::BrokenFile(
                        "the top level must be an object".to_owned(),
                    ));
                }
                Err(error) => return Err(ConfigError::BrokenFile(error.to_string())),
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => Map::new(),
            Err(source) => return Err(self.io_error(source)),
        };
        apply(&mut root, &config);

        // Nothing to say and no file yet: do not create an empty one.
        if root.is_empty() && !self.path.exists() {
            return Ok(self.reload());
        }
        self.write(&root)?;
        Ok(self.reload())
    }

    /// Creates an empty config so there is something to open in an editor.
    pub fn ensure_file(&self) -> Result<(), ConfigError> {
        if self.path.exists() {
            return Ok(());
        }
        self.write(&Map::new())?;
        self.reload();
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn reload(&self) -> ConfigSnapshot {
        let mut loaded = self.lock();
        *loaded = load(&self.path, &self.home);
        loaded.snapshot.clone()
    }

    /// Temp file plus rename, so a crash never leaves half a config behind.
    fn write(&self, root: &Map<String, Value>) -> Result<(), ConfigError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|source| self.io_error(source))?;
        }
        let mut text = serde_json::to_string_pretty(root)
            .map_err(|error| self.io_error(io::Error::other(error)))?;
        text.push('\n');
        let temporary = self.path.with_extension("json.tmp");
        fs::write(&temporary, text).map_err(|source| self.io_error(source))?;
        fs::rename(&temporary, &self.path).map_err(|source| self.io_error(source))
    }

    fn io_error(&self, source: io::Error) -> ConfigError {
        ConfigError::Io {
            path: self.path.clone(),
            source,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Loaded> {
        // The state is replaced whole, so a poisoned lock holds nothing half-written.
        self.loaded.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn fingerprint(path: &Path) -> Fingerprint {
    let metadata = fs::metadata(path).ok()?;
    Some((metadata.modified().ok()?, metadata.len()))
}

fn load(path: &Path, home: &Path) -> Loaded {
    let fingerprint = fingerprint(path);
    let path_text = path.to_string_lossy().into_owned();

    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) => {
            let (issues, file_exists) = if error.kind() == io::ErrorKind::NotFound {
                (Vec::new(), false)
            } else {
                (
                    vec![ConfigIssue {
                        key: String::new(),
                        message: format!("could not read the file: {error}"),
                    }],
                    true,
                )
            };
            return Loaded {
                snapshot: ConfigSnapshot {
                    config: AppConfig::default(),
                    issues,
                    path: path_text,
                    file_exists,
                    file_broken: file_exists,
                },
                fingerprint,
            };
        }
    };

    // A file with only whitespace is a file the user has not filled in yet.
    let parsed = if text.trim().is_empty() {
        Ok(json!({}))
    } else {
        serde_json::from_str::<Value>(&text)
    };

    let snapshot = match parsed {
        Ok(value) => {
            let (config, issues) = parse_config(&value, home);
            let broken = !value.is_object();
            ConfigSnapshot {
                config,
                issues,
                path: path_text,
                file_exists: true,
                file_broken: broken,
            }
        }
        Err(error) => ConfigSnapshot {
            config: AppConfig::default(),
            issues: vec![ConfigIssue {
                key: String::new(),
                message: format!("not valid JSON: {error}"),
            }],
            path: path_text,
            file_exists: true,
            file_broken: true,
        },
    };
    Loaded {
        snapshot,
        fingerprint,
    }
}

/// Writes `config` into `root`: a key is present exactly when it differs from the default.
fn apply(root: &mut Map<String, Value>, config: &AppConfig) {
    let terminal = &config.terminal;
    put(
        root,
        "terminal",
        "fontFamily",
        terminal.font_family.as_ref().map(|v| json!(v)),
    );
    put(
        root,
        "terminal",
        "fontSize",
        (terminal.font_size != DEFAULT_FONT_SIZE).then(|| json!(terminal.font_size)),
    );
    put(
        root,
        "terminal",
        "shell",
        terminal.shell.as_ref().map(|v| json!(v)),
    );
    put(
        root,
        "terminal",
        "scrollback",
        (terminal.scrollback != DEFAULT_SCROLLBACK).then(|| json!(terminal.scrollback)),
    );
    let defaults = TerminalConfig::default();
    put(
        root,
        "terminal",
        "copyOnSelect",
        (terminal.copy_on_select != defaults.copy_on_select)
            .then(|| json!(terminal.copy_on_select)),
    );
    put(
        root,
        "terminal",
        "bell",
        (terminal.bell != defaults.bell).then(|| json!(terminal.bell)),
    );
    put(
        root,
        "terminal",
        "macOptionIsMeta",
        (terminal.mac_option_is_meta != defaults.mac_option_is_meta)
            .then(|| json!(terminal.mac_option_is_meta)),
    );
    put(
        root,
        "terminal",
        "closeOnExit",
        (terminal.close_on_exit != defaults.close_on_exit).then(|| json!(terminal.close_on_exit)),
    );
    put(
        root,
        "terminal",
        "confirmClose",
        (terminal.confirm_close != defaults.confirm_close).then(|| json!(terminal.confirm_close)),
    );
    put(
        root,
        "terminal",
        "colorScheme",
        (terminal.color_scheme != defaults.color_scheme).then(|| json!(terminal.color_scheme)),
    );
    put(
        root,
        "terminal",
        "textColorScheme",
        terminal.text_color_scheme.map(|scheme| json!(scheme)),
    );
    put(
        root,
        "worktrees",
        "directory",
        config.worktrees.directory.as_ref().map(|v| json!(v)),
    );
    put(
        root,
        "agents",
        "launchMode",
        (config.agents.launch_mode != LaunchMode::Manual).then(|| json!("yolo")),
    );
    put_launch_arguments(root, &config.agents.launch_arguments);
    let notifications = &config.notifications;
    let notification_defaults = NotificationsConfig::default();
    put(
        root,
        "notifications",
        "onDone",
        (notifications.on_done != notification_defaults.on_done)
            .then(|| json!(notifications.on_done)),
    );
    put(
        root,
        "notifications",
        "onNeedsYou",
        (notifications.on_needs_you != notification_defaults.on_needs_you)
            .then(|| json!(notifications.on_needs_you)),
    );
    let git = &config.git;
    let git_defaults = GitConfig::default();
    put(
        root,
        "git",
        "commitMessageAgent",
        (git.commit_message_agent != git_defaults.commit_message_agent)
            .then(|| json!(git.commit_message_agent)),
    );
    put(
        root,
        "git",
        "commitMessageStyle",
        (git.commit_message_style != git_defaults.commit_message_style)
            .then(|| json!(git.commit_message_style)),
    );
    put(
        root,
        "git",
        "commitMessageInstructions",
        git.commit_message_instructions
            .as_ref()
            .map(|text| json!(text)),
    );
    let browser = &config.browser;
    let browser_defaults = BrowserConfig::default();
    put(
        root,
        "browser",
        "agentAccess",
        (browser.agent_access != browser_defaults.agent_access)
            .then(|| json!(browser.agent_access)),
    );
    put(
        root,
        "browser",
        "allowedSites",
        (!browser.allowed_sites.is_empty()).then(|| json!(browser.allowed_sites)),
    );
}

/// Writes each agent's arguments under `agents.launchArguments`. Only the names Lynx knows are
/// rewritten, so an entry for an agent this build does not know stays as the user wrote it.
fn put_launch_arguments(root: &mut Map<String, Value>, arguments: &BTreeMap<HarnessId, String>) {
    let agents = root.entry("agents").or_insert_with(|| json!({}));
    if !agents.is_object() {
        // Already reported as invalid; the section is ours to rewrite.
        *agents = json!({});
    }
    let Some(agents) = agents.as_object_mut() else {
        return;
    };
    let list = agents.entry("launchArguments").or_insert_with(|| json!({}));
    if !list.is_object() {
        *list = json!({});
    }
    let Some(list) = list.as_object_mut() else {
        return;
    };
    for harness in supported() {
        list.shift_remove(harness_name(harness));
    }
    for (harness, text) in arguments {
        list.insert(harness_name(*harness).to_owned(), json!(text));
    }
    if list.is_empty() {
        agents.shift_remove("launchArguments");
    }
    if agents.is_empty() {
        root.shift_remove("agents");
    }
}

fn put(root: &mut Map<String, Value>, section: &str, key: &str, value: Option<Value>) {
    match value {
        Some(value) => {
            let entry = root
                .entry(section)
                .or_insert_with(|| Value::Object(Map::new()));
            if !entry.is_object() {
                // Already reported as invalid; the section is ours to rewrite.
                *entry = Value::Object(Map::new());
            }
            if let Some(object) = entry.as_object_mut() {
                object.insert(key.to_owned(), value);
            }
        }
        None => {
            let Some(object) = root.get_mut(section).and_then(Value::as_object_mut) else {
                return;
            };
            object.shift_remove(key);
            if object.is_empty() {
                root.shift_remove(section);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::thread::sleep;
    use std::time::Duration;

    use super::*;
    use crate::config::app_config::{
        BellStyle, ColorScheme, CommitMessageAgent, CommitMessageStyle, TerminalConfig,
        WorktreesConfig,
    };

    fn setup() -> (ConfigStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join(".lynx").join("config.json");
        (ConfigStore::open(path, PathBuf::from("/home/dev")), dir)
    }

    fn with_font_size(size: u8) -> AppConfig {
        AppConfig {
            terminal: TerminalConfig {
                font_size: size,
                ..TerminalConfig::default()
            },
            ..AppConfig::default()
        }
    }

    fn file_json(store: &ConfigStore) -> Value {
        serde_json::from_str(&fs::read_to_string(store.path()).expect("read")).expect("json")
    }

    #[test]
    fn a_missing_file_gives_defaults_and_is_not_created_by_reading() {
        let (store, _dir) = setup();
        let snapshot = store.snapshot();
        assert_eq!(snapshot.config, AppConfig::default());
        assert!(!snapshot.file_exists && !snapshot.file_broken && snapshot.issues.is_empty());
        assert!(!store.path().exists());
    }

    #[test]
    fn saving_creates_the_folder_and_writes_only_non_default_values() {
        let (store, _dir) = setup();

        let snapshot = store.update(with_font_size(16)).expect("update");

        assert_eq!(file_json(&store), json!({ "terminal": { "fontSize": 16 } }));
        assert_eq!(snapshot.config.terminal.font_size, 16);
        assert!(snapshot.file_exists);
    }

    #[test]
    fn saving_all_defaults_when_there_is_no_file_creates_nothing() {
        let (store, _dir) = setup();
        store.update(AppConfig::default()).expect("update");
        assert!(!store.path().exists());
    }

    #[test]
    fn going_back_to_a_default_removes_the_key_and_the_empty_section() {
        let (store, _dir) = setup();
        store.update(with_font_size(16)).expect("set");

        store
            .update(with_font_size(DEFAULT_FONT_SIZE))
            .expect("reset");

        assert_eq!(file_json(&store), json!({}));
    }

    #[test]
    fn terminal_behaviour_options_are_written_only_when_they_differ_from_defaults() {
        let (store, _dir) = setup();
        let changed = AppConfig {
            terminal: TerminalConfig {
                copy_on_select: true,
                bell: BellStyle::None,
                confirm_close: true,
                ..TerminalConfig::default()
            },
            ..AppConfig::default()
        };

        store.update(changed).expect("set");
        assert_eq!(
            file_json(&store),
            json!({ "terminal": { "copyOnSelect": true, "bell": "none", "confirmClose": true } })
        );

        store.update(AppConfig::default()).expect("reset");
        assert_eq!(file_json(&store), json!({}));
    }

    #[test]
    fn a_turned_off_notification_is_written_and_back_on_removes_the_key() {
        let (store, _dir) = setup();
        let quiet = AppConfig {
            notifications: NotificationsConfig {
                on_done: false,
                ..NotificationsConfig::default()
            },
            ..AppConfig::default()
        };

        store.update(quiet).expect("set");
        assert_eq!(
            file_json(&store),
            json!({ "notifications": { "onDone": false } })
        );

        store.update(AppConfig::default()).expect("reset");
        assert_eq!(file_json(&store), json!({}));
    }

    #[test]
    fn theme_settings_are_written_only_when_set() {
        let (store, _dir) = setup();
        let themed = AppConfig {
            terminal: TerminalConfig {
                color_scheme: ColorScheme::Nord,
                text_color_scheme: Some(ColorScheme::Dracula),
                ..TerminalConfig::default()
            },
            ..AppConfig::default()
        };

        store.update(themed).expect("set");
        assert_eq!(
            file_json(&store),
            json!({ "terminal": { "colorScheme": "nord", "textColorScheme": "dracula" } })
        );

        store.update(AppConfig::default()).expect("reset");
        assert_eq!(file_json(&store), json!({}));
    }

    #[test]
    fn saving_keeps_unknown_keys_comments_free_order_and_neighbors() {
        let (store, _dir) = setup();
        fs::create_dir_all(store.path().parent().expect("parent")).expect("mkdir");
        fs::write(
            store.path(),
            r#"{ "zeta": { "keep": [1, 2] }, "terminal": { "other": true, "fontSize": 20 }, "alpha": "x" }"#,
        )
        .expect("write");
        store.reload_if_changed();

        store
            .update(AppConfig {
                terminal: TerminalConfig {
                    font_size: 18,
                    shell: Some("/bin/fish".to_owned()),
                    ..TerminalConfig::default()
                },
                ..AppConfig::default()
            })
            .expect("update");

        let saved = file_json(&store);
        assert_eq!(
            saved,
            json!({ "zeta": { "keep": [1, 2] }, "terminal": { "other": true, "fontSize": 18, "shell": "/bin/fish" }, "alpha": "x" })
        );
        let order: Vec<_> = saved.as_object().expect("object").keys().collect();
        assert_eq!(order, ["zeta", "terminal", "alpha"]);
    }

    #[test]
    fn invalid_values_are_refused_and_the_file_is_untouched() {
        let (store, _dir) = setup();
        store.update(with_font_size(16)).expect("set");
        let before = fs::read_to_string(store.path()).expect("read");

        let result = store.update(with_font_size(99));

        assert!(
            matches!(&result, Err(ConfigError::InvalidValues(issues)) if issues[0].key == "terminal.fontSize")
        );
        assert_eq!(fs::read_to_string(store.path()).expect("read"), before);
    }

    #[test]
    fn text_is_trimmed_and_blank_text_clears_the_setting() {
        let (store, _dir) = setup();
        store
            .update(AppConfig {
                terminal: TerminalConfig {
                    shell: Some("  /bin/fish  ".to_owned()),
                    ..TerminalConfig::default()
                },
                ..AppConfig::default()
            })
            .expect("set");
        assert_eq!(store.config().terminal.shell.as_deref(), Some("/bin/fish"));

        store
            .update(AppConfig {
                terminal: TerminalConfig {
                    shell: Some("   ".to_owned()),
                    ..TerminalConfig::default()
                },
                ..AppConfig::default()
            })
            .expect("clear");

        assert_eq!(store.config().terminal.shell, None);
        assert_eq!(file_json(&store), json!({}));
    }

    #[test]
    fn a_file_that_is_not_json_is_reported_and_never_overwritten() {
        let (store, _dir) = setup();
        fs::create_dir_all(store.path().parent().expect("parent")).expect("mkdir");
        fs::write(store.path(), "{ \"terminal\": ").expect("write");
        let snapshot = store.reload_if_changed().expect("changed");

        assert!(snapshot.file_broken);
        assert_eq!(snapshot.config, AppConfig::default());
        assert!(snapshot.issues[0].message.contains("not valid JSON"));

        let result = store.update(with_font_size(16));
        assert!(matches!(result, Err(ConfigError::BrokenFile(_))));
        assert_eq!(
            fs::read_to_string(store.path()).expect("read"),
            "{ \"terminal\": "
        );
    }

    #[test]
    fn an_empty_file_is_treated_as_empty_settings_not_as_broken() {
        let (store, _dir) = setup();
        fs::create_dir_all(store.path().parent().expect("parent")).expect("mkdir");
        fs::write(store.path(), "  \n").expect("write");

        let snapshot = store.reload_if_changed().expect("changed");

        assert!(!snapshot.file_broken && snapshot.issues.is_empty());
        store
            .update(with_font_size(15))
            .expect("can save over an empty file");
    }

    #[test]
    fn invalid_fields_in_the_file_are_reported_but_the_rest_loads() {
        let (store, _dir) = setup();
        fs::create_dir_all(store.path().parent().expect("parent")).expect("mkdir");
        fs::write(
            store.path(),
            r#"{ "terminal": { "fontSize": 2, "shell": "/bin/zsh" } }"#,
        )
        .expect("write");

        let snapshot = store.reload_if_changed().expect("changed");

        assert_eq!(snapshot.issues.len(), 1);
        assert_eq!(snapshot.config.terminal.font_size, DEFAULT_FONT_SIZE);
        assert_eq!(snapshot.config.terminal.shell.as_deref(), Some("/bin/zsh"));
        assert!(!snapshot.file_broken);
    }

    #[test]
    fn outside_edits_are_noticed_once() {
        let (store, _dir) = setup();
        store.update(with_font_size(16)).expect("set");
        assert!(
            store.reload_if_changed().is_none(),
            "our own write is not news"
        );

        sleep(Duration::from_millis(20));
        fs::write(
            store.path(),
            r#"{ "terminal": { "fontSize": 22, "scrollback": 100 } }"#,
        )
        .expect("edit");

        let snapshot = store.reload_if_changed().expect("noticed");
        assert_eq!(snapshot.config.terminal.font_size, 22);
        assert_eq!(snapshot.config.terminal.scrollback, 100);
        assert!(store.reload_if_changed().is_none());
    }

    #[test]
    fn deleting_the_file_returns_to_defaults() {
        let (store, _dir) = setup();
        store.update(with_font_size(16)).expect("set");

        fs::remove_file(store.path()).expect("delete");
        let snapshot = store.reload_if_changed().expect("noticed");

        assert_eq!(snapshot.config, AppConfig::default());
        assert!(!snapshot.file_exists);
    }

    #[test]
    fn the_worktrees_directory_expands_the_home_prefix() {
        let (store, _dir) = setup();
        assert_eq!(store.worktrees_directory(), None);

        store
            .update(AppConfig {
                worktrees: WorktreesConfig {
                    directory: Some("~/trees".to_owned()),
                },
                ..AppConfig::default()
            })
            .expect("set");

        assert_eq!(
            store.worktrees_directory(),
            Some(PathBuf::from("/home/dev/trees"))
        );
    }

    #[test]
    fn ensure_file_creates_an_empty_object_once_and_keeps_existing_content() {
        let (store, _dir) = setup();
        store.ensure_file().expect("create");
        assert_eq!(file_json(&store), json!({}));

        store.update(with_font_size(16)).expect("set");
        store.ensure_file().expect("again");
        assert_eq!(file_json(&store), json!({ "terminal": { "fontSize": 16 } }));
    }

    #[test]
    fn launch_arguments_are_written_per_agent_and_only_when_set() {
        let (store, _dir) = setup();
        let mut config = AppConfig::default();
        config
            .agents
            .launch_arguments
            .insert(HarnessId::Claude, "--model opus".to_owned());

        store.update(config).expect("update");

        assert_eq!(
            file_json(&store),
            json!({ "agents": { "launchArguments": { "claude": "--model opus" } } })
        );
    }

    #[test]
    fn launch_arguments_keep_the_entries_of_agents_this_build_does_not_know() {
        let (store, _dir) = setup();
        fs::create_dir_all(store.path().parent().expect("parent")).expect("mkdir");
        fs::write(
            store.path(),
            r#"{ "agents": { "launchArguments": { "mystery": "--x", "codex": "--old" } } }"#,
        )
        .expect("write");
        store.reload_if_changed();
        let mut config = AppConfig::default();
        config
            .agents
            .launch_arguments
            .insert(HarnessId::Claude, "--model opus".to_owned());

        store.update(config).expect("update");

        assert_eq!(
            file_json(&store),
            json!({ "agents": { "launchArguments": { "mystery": "--x", "claude": "--model opus" } } })
        );
    }

    #[test]
    fn clearing_the_arguments_removes_the_section_when_nothing_else_is_in_it() {
        let (store, _dir) = setup();
        let mut config = AppConfig::default();
        config
            .agents
            .launch_arguments
            .insert(HarnessId::Codex, "--full-auto".to_owned());
        store.update(config).expect("set");

        store.update(AppConfig::default()).expect("clear");

        assert_eq!(file_json(&store), json!({}));
    }

    #[test]
    fn the_launch_mode_is_written_only_when_it_is_yolo() {
        let (store, _dir) = setup();
        let mut config = AppConfig::default();
        config.agents.launch_mode = LaunchMode::Yolo;

        store.update(config).expect("update");
        assert_eq!(
            file_json(&store),
            json!({ "agents": { "launchMode": "yolo" } })
        );

        store.update(AppConfig::default()).expect("back to manual");
        assert_eq!(file_json(&store), json!({}));
    }

    #[test]
    fn an_argument_entry_with_no_text_is_written_as_on() {
        let (store, _dir) = setup();
        let mut config = AppConfig::default();
        config
            .agents
            .launch_arguments
            .insert(HarnessId::Codex, String::new());

        store.update(config).expect("update");

        assert_eq!(
            file_json(&store),
            json!({ "agents": { "launchArguments": { "codex": "" } } })
        );
    }

    #[test]
    fn the_git_settings_are_written_only_when_they_differ_from_defaults() {
        let (store, _dir) = setup();
        let mut config = AppConfig::default();
        config.git.commit_message_agent = CommitMessageAgent::Claude;
        config.git.commit_message_style = CommitMessageStyle::Custom;
        config.git.commit_message_instructions = Some("Write in Portuguese.".to_owned());

        store.update(config).expect("update");
        assert_eq!(
            file_json(&store),
            json!({
                "git": {
                    "commitMessageAgent": "claude",
                    "commitMessageStyle": "custom",
                    "commitMessageInstructions": "Write in Portuguese."
                }
            })
        );

        store
            .update(AppConfig::default())
            .expect("back to defaults");
        assert_eq!(file_json(&store), json!({}));
    }

    #[test]
    fn the_browser_settings_are_written_only_when_they_differ_from_defaults() {
        let (store, _dir) = setup();
        let mut config = AppConfig::default();
        config.browser.agent_access = crate::browser::AgentBrowserAccess::ReadOnly;
        config.browser.allowed_sites = vec!["example.com".to_owned()];

        store.update(config).expect("update");
        assert_eq!(
            file_json(&store),
            json!({ "browser": { "agentAccess": "readOnly", "allowedSites": ["example.com"] } })
        );

        store
            .update(AppConfig::default())
            .expect("back to defaults");
        assert_eq!(file_json(&store), json!({}));
    }
}
