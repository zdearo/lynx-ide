//! Claude Code hooks: the entries Lynx adds to Claude's settings so it hears about each turn.
//! The file is only changed on request, and entries that are not Lynx's are never touched.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::activity::HookEvent;
use super::hook_install::{
    HookInstallError, HookInstallState, HookInstallStatus, Version, relay_command,
};
use super::json_hook_file::{self, EventSpec, JsonHookFormat};

/// Appended to the app's executable path; a hook command carrying it is Lynx's.
const MARKER: &str = " --agent-hook claude";

/// Used when the installed version cannot be read. Claude 1.0.23 through 2.1.100 discard their
/// whole settings file over one event name they do not know, so an unknown version gets only the
/// events that every release from this one on accepts.
const UNRESOLVED_VERSION: Version = [1, 0, 64];
const UNRESOLVED_NOTE: &str =
    "Could not read the Claude version, so only the events every release accepts are used.";

const FORMAT: JsonHookFormat = JsonHookFormat {
    root_key: "hooks",
    version: None,
    event_argument: false,
    handler: claude_handler,
};

fn claude_handler(command: &str) -> Value {
    json!({ "type": "command", "command": command })
}

/// An event with the first Claude release that accepts it and delivers it. SessionEnd is known
/// from 1.0.85, but older releases did not deliver it reliably, so it waits for 2.1.261.
struct HookedEvent {
    spec: EventSpec,
    from: Version,
}

const HOOKED_EVENTS: &[HookedEvent] = &[
    HookedEvent {
        spec: EventSpec::grouped("SessionStart", None),
        from: [1, 0, 62],
    },
    HookedEvent {
        spec: EventSpec::grouped("UserPromptSubmit", None),
        from: [1, 0, 53],
    },
    HookedEvent {
        spec: EventSpec::grouped("PreToolUse", Some("*")),
        from: [1, 0, 23],
    },
    HookedEvent {
        spec: EventSpec::grouped("PostToolUse", Some("*")),
        from: [1, 0, 23],
    },
    HookedEvent {
        spec: EventSpec::grouped("PostToolUseFailure", Some("*")),
        from: [2, 0, 56],
    },
    HookedEvent {
        spec: EventSpec::grouped("PermissionRequest", Some("*")),
        from: [2, 0, 45],
    },
    HookedEvent {
        spec: EventSpec::grouped("Stop", None),
        from: [1, 0, 31],
    },
    HookedEvent {
        spec: EventSpec::grouped("StopFailure", None),
        from: [2, 1, 78],
    },
    HookedEvent {
        spec: EventSpec::grouped("SessionEnd", None),
        from: [2, 1, 261],
    },
];

/// The events to write for this Claude version.
fn events_for(version: Option<Version>) -> Vec<EventSpec> {
    let version = version.unwrap_or(UNRESOLVED_VERSION);
    HOOKED_EVENTS
        .iter()
        .filter(|event| version >= event.from)
        .map(|event| event.spec)
        .collect()
}

/// Where Claude keeps its settings: `CLAUDE_CONFIG_DIR` when set, else `~/.claude`.
pub fn settings_path(config_dir: Option<PathBuf>, home: &Path) -> PathBuf {
    config_dir
        .unwrap_or_else(|| home.join(".claude"))
        .join("settings.json")
}

/// The command Claude runs for each event: the app's own executable in relay mode.
pub fn hook_command(executable: &Path) -> String {
    relay_command(executable, "claude")
}

/// Translates a hook payload from Claude into the event it stands for, or None when Lynx does
/// not track it.
pub fn event_from_payload(payload: &Value) -> Option<HookEvent> {
    match payload.get("hook_event_name")?.as_str()? {
        "SessionStart" => match payload.get("source")?.as_str()? {
            // A compact restart fires mid-turn, so only the idle sources count.
            "startup" | "resume" | "clear" => Some(HookEvent::Started),
            _ => None,
        },
        "UserPromptSubmit" | "PostToolUse" | "PostToolUseFailure" => Some(HookEvent::Working),
        "PreToolUse" => Some(
            if payload.get("tool_name").and_then(Value::as_str) == Some("AskUserQuestion") {
                HookEvent::NeedsYou
            } else {
                HookEvent::Working
            },
        ),
        "PermissionRequest" => Some(HookEvent::NeedsYou),
        "Stop" | "StopFailure" => Some(HookEvent::Finished),
        "SessionEnd" => Some(HookEvent::Ended),
        _ => None,
    }
}

/// The Claude settings file for this app, and the command that the hooks should run.
#[derive(Clone)]
pub struct ClaudeHookTarget {
    settings_path: PathBuf,
    command: Option<String>,
}

impl ClaudeHookTarget {
    pub fn new(settings_path: PathBuf, executable: Option<&Path>) -> Self {
        Self {
            settings_path,
            command: executable.map(hook_command),
        }
    }

    pub fn status(&self, version: Option<Version>) -> HookInstallStatus {
        match read_settings(&self.settings_path) {
            Ok(settings) => self.describe(&settings, version),
            Err(error) => {
                self.status_with(version, HookInstallState::Error, Some(error.to_string()))
            }
        }
    }

    /// Adds Lynx's entries, replacing any it wrote before (for example from an older build).
    pub fn install(&self, version: Option<Version>) -> Result<HookInstallStatus, HookInstallError> {
        let command = self
            .command
            .as_deref()
            .ok_or(HookInstallError::NoExecutable)?;
        let mut settings = read_settings(&self.settings_path)?;
        apply(&mut settings, command, &events_for(version));
        write_settings(&self.settings_path, &settings)?;
        Ok(self.status(version))
    }

    /// Removes Lynx's entries and leaves the rest of the file as it was.
    pub fn remove(&self, version: Option<Version>) -> Result<HookInstallStatus, HookInstallError> {
        let mut settings = read_settings(&self.settings_path)?;
        if strip(&mut settings) {
            write_settings(&self.settings_path, &settings)?;
        }
        Ok(self.status(version))
    }

    fn describe(&self, settings: &Value, version: Option<Version>) -> HookInstallStatus {
        let Some(command) = self.command.as_deref() else {
            return self.status_with(
                version,
                HookInstallState::Error,
                Some(HookInstallError::NoExecutable.to_string()),
            );
        };
        let events = events_for(version);
        let missing = json_hook_file::missing_events(settings, &FORMAT, command, &events);
        let state = if missing.is_empty() {
            HookInstallState::Installed
        } else if missing.len() == events.len() {
            HookInstallState::NotInstalled
        } else {
            HookInstallState::Partial
        };
        let mut notes = Vec::new();
        if version.is_none() {
            notes.push(UNRESOLVED_NOTE.to_owned());
        }
        if state == HookInstallState::Partial {
            notes.push(format!("Missing hooks for: {}", missing.join(", ")));
        }
        let detail = (!notes.is_empty()).then(|| notes.join(" "));
        self.status_with(version, state, detail)
    }

    fn status_with(
        &self,
        version: Option<Version>,
        state: HookInstallState,
        detail: Option<String>,
    ) -> HookInstallStatus {
        HookInstallStatus {
            state,
            config_path: self.settings_path.to_string_lossy().into_owned(),
            events: events_for(version)
                .iter()
                .map(|event| event.name.to_owned())
                .collect(),
            detail,
        }
    }
}

fn read_settings(path: &Path) -> Result<Value, HookInstallError> {
    json_hook_file::read_settings(path, &FORMAT)
}

fn write_settings(path: &Path, settings: &Value) -> Result<(), HookInstallError> {
    json_hook_file::write_settings(path, settings)
}

fn apply(settings: &mut Value, command: &str, events: &[EventSpec]) {
    json_hook_file::apply(settings, &FORMAT, MARKER, command, events);
}

fn strip(settings: &mut Value) -> bool {
    json_hook_file::strip(settings, &FORMAT, MARKER)
}

#[cfg(test)]
fn hook_commands<'a>(settings: &'a Value, event: &str) -> Vec<&'a str> {
    json_hook_file::commands_of(settings, &FORMAT, event)
}

#[cfg(test)]
fn valid_shape(settings: &Value) -> bool {
    json_hook_file::valid_shape(settings, &FORMAT)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    const COMMAND: &str = "'/opt/lynx/lynx-ide' --agent-hook claude";
    /// A version that accepts every event.
    const RECENT: Version = [9, 9, 9];

    fn user_hook(command: &str) -> Value {
        json!({ "hooks": [{ "type": "command", "command": command }] })
    }

    fn names(events: &[EventSpec]) -> Vec<&'static str> {
        events.iter().map(|event| event.name).collect()
    }

    #[test]
    fn install_adds_one_entry_per_event_for_the_version() {
        let mut settings = json!({});
        let events = events_for(Some(RECENT));
        apply(&mut settings, COMMAND, &events);
        for event in &events {
            assert_eq!(
                hook_commands(&settings, event.name),
                vec![COMMAND],
                "{}",
                event.name
            );
        }
    }

    #[test]
    fn tool_events_get_the_wildcard_matcher_and_the_others_do_not() {
        let mut settings = json!({});
        apply(&mut settings, COMMAND, &events_for(Some(RECENT)));
        assert_eq!(settings["hooks"]["PreToolUse"][0]["matcher"], "*");
        assert!(settings["hooks"]["Stop"][0].get("matcher").is_none());
    }

    #[test]
    fn install_keeps_the_users_own_settings_and_hooks() {
        let mut settings = json!({
            "model": "opus",
            "hooks": { "Stop": [user_hook("notify-send done")] }
        });
        apply(&mut settings, COMMAND, &events_for(Some(RECENT)));
        assert_eq!(settings["model"], "opus");
        assert_eq!(
            hook_commands(&settings, "Stop"),
            vec!["notify-send done", COMMAND]
        );
    }

    #[test]
    fn install_replaces_an_older_lynx_entry_instead_of_duplicating_it() {
        let mut settings = json!({});
        apply(
            &mut settings,
            "'/old/lynx-ide' --agent-hook claude",
            &events_for(Some(RECENT)),
        );
        apply(&mut settings, COMMAND, &events_for(Some(RECENT)));
        assert_eq!(hook_commands(&settings, "Stop"), vec![COMMAND]);
    }

    #[test]
    fn remove_takes_only_lynx_entries_and_leaves_the_rest() {
        let mut settings = json!({
            "hooks": { "Stop": [user_hook("notify-send done")] }
        });
        apply(&mut settings, COMMAND, &events_for(Some(RECENT)));
        assert!(strip(&mut settings));
        assert_eq!(
            settings,
            json!({ "hooks": { "Stop": [user_hook("notify-send done")] } })
        );
    }

    #[test]
    fn remove_drops_the_hooks_object_when_lynx_was_its_only_content() {
        let mut settings = json!({ "model": "opus" });
        apply(&mut settings, COMMAND, &events_for(Some(RECENT)));
        assert!(strip(&mut settings));
        assert_eq!(settings, json!({ "model": "opus" }));
    }

    #[test]
    fn remove_on_a_file_without_lynx_reports_no_change() {
        let mut settings = json!({ "hooks": { "Stop": [user_hook("notify-send done")] } });
        let before = settings.clone();
        assert!(!strip(&mut settings));
        assert_eq!(settings, before);
    }

    #[test]
    fn a_settings_file_that_is_not_an_object_is_refused() {
        assert!(!valid_shape(&json!([])));
        assert!(!valid_shape(&json!({ "hooks": [] })));
        assert!(!valid_shape(&json!({ "hooks": { "Stop": {} } })));
        assert!(valid_shape(&json!({})));
    }

    #[test]
    fn status_reports_installed_partial_and_not_installed() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("settings.json");
        let target = ClaudeHookTarget {
            settings_path: path.clone(),
            command: Some(COMMAND.to_owned()),
        };
        assert_eq!(
            target.status(Some(RECENT)).state,
            HookInstallState::NotInstalled
        );

        target.install(Some(RECENT)).expect("install");
        assert_eq!(
            target.status(Some(RECENT)).state,
            HookInstallState::Installed
        );

        let mut settings: Value =
            serde_json::from_str(&fs::read_to_string(&path).expect("read")).expect("json");
        settings["hooks"]
            .as_object_mut()
            .expect("hooks")
            .remove("Stop");
        fs::write(&path, settings.to_string()).expect("write");
        let status = target.status(Some(RECENT));
        assert_eq!(status.state, HookInstallState::Partial);
        assert!(status.detail.unwrap_or_default().contains("Stop"));
    }

    #[test]
    fn install_then_remove_round_trips_through_the_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("nested").join("settings.json");
        let target = ClaudeHookTarget {
            settings_path: path.clone(),
            command: Some(COMMAND.to_owned()),
        };
        target.install(Some(RECENT)).expect("install");
        assert_eq!(
            target.status(Some(RECENT)).state,
            HookInstallState::Installed
        );
        assert!(
            !path.with_extension("json.tmp").exists(),
            "temp file is renamed away"
        );

        target.remove(Some(RECENT)).expect("remove");
        assert_eq!(
            target.status(Some(RECENT)).state,
            HookInstallState::NotInstalled
        );
    }

    #[test]
    fn a_file_that_is_not_json_is_reported_and_never_overwritten() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("settings.json");
        fs::write(&path, "{ not json").expect("write");
        let target = ClaudeHookTarget {
            settings_path: path.clone(),
            command: Some(COMMAND.to_owned()),
        };
        assert_eq!(target.status(None).state, HookInstallState::Error);
        assert!(target.install(None).is_err());
        assert_eq!(fs::read_to_string(&path).expect("read"), "{ not json");
    }

    #[test]
    fn install_without_an_executable_refuses_to_write() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("settings.json");
        let target = ClaudeHookTarget {
            settings_path: path.clone(),
            command: None,
        };
        assert!(matches!(
            target.install(Some(RECENT)),
            Err(HookInstallError::NoExecutable)
        ));
        assert!(!path.exists());
    }

    /// The bug this guards against: an old Claude discards its whole settings over one unknown
    /// event, so Lynx must not write events that version does not know.
    #[test]
    fn an_old_claude_is_only_given_events_it_knows() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("settings.json");
        fs::write(&path, r#"{"model":"opus"}"#).expect("write");
        let target = ClaudeHookTarget {
            settings_path: path.clone(),
            command: Some(COMMAND.to_owned()),
        };

        let status = target.install(Some([1, 0, 70])).expect("install");

        let settings: Value =
            serde_json::from_str(&fs::read_to_string(&path).expect("read")).expect("json");
        assert_eq!(settings["model"], "opus", "the user's settings survive");
        assert!(settings["hooks"].get("PermissionRequest").is_none());
        assert!(settings["hooks"].get("SessionEnd").is_none());
        assert!(settings["hooks"].get("SessionStart").is_some());
        assert!(!status.events.iter().any(|name| name == "SessionEnd"));
        assert_eq!(status.state, HookInstallState::Installed);
    }

    /// A connection made before SessionEnd existed is partial; updating it writes the missing event.
    #[test]
    fn updating_a_partial_connection_adds_the_missing_event() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("settings.json");
        let target = ClaudeHookTarget {
            settings_path: path.clone(),
            command: Some(COMMAND.to_owned()),
        };
        target.install(Some(RECENT)).expect("install");
        let mut settings: Value =
            serde_json::from_str(&fs::read_to_string(&path).expect("read")).expect("json");
        settings["hooks"]
            .as_object_mut()
            .expect("hooks")
            .remove("SessionEnd");
        fs::write(&path, settings.to_string()).expect("write");
        assert_eq!(target.status(Some(RECENT)).state, HookInstallState::Partial);

        let status = target.install(Some(RECENT)).expect("update");

        assert_eq!(status.state, HookInstallState::Installed);
        assert_eq!(
            hook_commands(
                &serde_json::from_str::<Value>(&fs::read_to_string(&path).expect("read"))
                    .expect("json"),
                "SessionEnd"
            ),
            vec![COMMAND]
        );
    }

    #[test]
    fn session_end_waits_for_2_1_261() {
        assert!(!names(&events_for(Some([2, 1, 260]))).contains(&"SessionEnd"));
        assert!(names(&events_for(Some([2, 1, 261]))).contains(&"SessionEnd"));
    }

    #[test]
    fn an_unknown_version_gets_the_conservative_set() {
        assert_eq!(
            names(&events_for(None)),
            names(&events_for(Some(UNRESOLVED_VERSION)))
        );
        assert!(!names(&events_for(None)).contains(&"PermissionRequest"));
    }

    #[test]
    fn an_unknown_version_is_said_in_the_status() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("settings.json");
        let target = ClaudeHookTarget {
            settings_path: path,
            command: Some(COMMAND.to_owned()),
        };
        target.install(None).expect("install");

        let status = target.status(None);

        assert_eq!(status.state, HookInstallState::Installed);
        assert!(status.detail.unwrap_or_default().contains("Could not read"));
    }

    #[cfg(not(windows))]
    #[test]
    fn hook_command_quotes_the_executable_path() {
        assert_eq!(
            hook_command(Path::new("/opt/it's here/lynx-ide")),
            "'/opt/it'\\''s here/lynx-ide' --agent-hook claude"
        );
    }

    #[test]
    fn settings_path_prefers_the_config_dir_override() {
        assert_eq!(
            settings_path(None, Path::new("/home/me")),
            PathBuf::from("/home/me/.claude/settings.json")
        );
        assert_eq!(
            settings_path(Some(PathBuf::from("/work/claude")), Path::new("/home/me")),
            PathBuf::from("/work/claude/settings.json")
        );
    }

    #[test]
    fn payloads_map_to_the_events_the_activity_tracks() {
        let event = |name: &str| json!({ "hook_event_name": name });
        assert_eq!(
            event_from_payload(&json!({ "hook_event_name": "SessionStart", "source": "startup" })),
            Some(HookEvent::Started)
        );
        assert_eq!(
            event_from_payload(&json!({ "hook_event_name": "SessionStart", "source": "compact" })),
            None
        );
        assert_eq!(
            event_from_payload(&event("UserPromptSubmit")),
            Some(HookEvent::Working)
        );
        assert_eq!(
            event_from_payload(&json!({ "hook_event_name": "PreToolUse", "tool_name": "Bash" })),
            Some(HookEvent::Working)
        );
        assert_eq!(
            event_from_payload(
                &json!({ "hook_event_name": "PreToolUse", "tool_name": "AskUserQuestion" })
            ),
            Some(HookEvent::NeedsYou)
        );
        assert_eq!(
            event_from_payload(&event("PermissionRequest")),
            Some(HookEvent::NeedsYou)
        );
        assert_eq!(
            event_from_payload(&event("Stop")),
            Some(HookEvent::Finished)
        );
        assert_eq!(
            event_from_payload(&event("SessionEnd")),
            Some(HookEvent::Ended)
        );
        assert_eq!(event_from_payload(&event("Notification")), None);
        assert_eq!(event_from_payload(&json!({})), None);
    }
}
