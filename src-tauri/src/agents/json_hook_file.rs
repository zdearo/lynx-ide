//! Hook entries in a JSON settings file, the shape most agents share. Lynx owns the entries whose
//! command carries its marker; every other entry and every other key stays as it is.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use super::hook_install::{
    HookInstallError, HookInstallState, HookInstallStatus, invalid_config, io_error, read_optional,
    relay_command, write_atomic,
};

/// One event Lynx listens to. A grouped event nests its handler in a group with an optional
/// matcher; a flat event lists the handler itself, with the matcher beside it when there is one.
#[derive(Debug, Clone, Copy)]
pub struct EventSpec {
    pub name: &'static str,
    pub grouped: bool,
    pub matcher: Option<&'static str>,
}

impl EventSpec {
    pub const fn grouped(name: &'static str, matcher: Option<&'static str>) -> Self {
        Self {
            name,
            grouped: true,
            matcher,
        }
    }

    pub const fn flat(name: &'static str, matcher: Option<&'static str>) -> Self {
        Self {
            name,
            grouped: false,
            matcher,
        }
    }
}

/// The file layout of one agent: where its events live and what a handler looks like.
#[derive(Debug, Clone, Copy)]
pub struct JsonHookFormat {
    /// The key holding the events: `hooks` for most agents, a bundle name for Antigravity.
    pub root_key: &'static str,
    /// A top-level version Lynx sets when the file has none, for agents that require one.
    pub version: Option<u64>,
    /// Copilot is told its event by an argument, so each event runs its own command.
    pub event_argument: bool,
    /// The handler object for one command.
    pub handler: fn(&str) -> Value,
}

/// The command one event runs: the base command, with the event as an argument where the agent
/// needs it.
fn command_for(format: &JsonHookFormat, base: &str, event: &str) -> String {
    if format.event_argument {
        format!("{base} {event}")
    } else {
        base.to_owned()
    }
}

/// The exact entry Lynx writes for one event.
fn entry(format: &JsonHookFormat, spec: &EventSpec, command: &str) -> Value {
    let mut handler = (format.handler)(command);
    if !spec.grouped {
        if let (Some(matcher), Some(object)) = (spec.matcher, handler.as_object_mut()) {
            object.insert("matcher".to_owned(), json!(matcher));
        }
        return handler;
    }
    match spec.matcher {
        Some(matcher) => json!({ "matcher": matcher, "hooks": [handler] }),
        None => json!({ "hooks": [handler] }),
    }
}

/// Whether a handler is Lynx's: one of its command fields carries the marker.
fn owned(handler: &Value, marker: &str) -> bool {
    ["command", "bash", "powershell"].iter().any(|key| {
        handler
            .get(*key)
            .and_then(Value::as_str)
            .is_some_and(|command| command.contains(marker))
    })
}

/// Whether the file has the layout this format needs, so a file of another shape is left alone.
pub fn valid_shape(settings: &Value, format: &JsonHookFormat) -> bool {
    let Some(root) = settings.as_object() else {
        return false;
    };
    match root.get(format.root_key) {
        None => true,
        Some(Value::Object(events)) => events.values().all(Value::is_array),
        Some(_) => false,
    }
}

/// Reads a settings file. A missing or empty file is empty settings.
pub fn read_settings(path: &Path, format: &JsonHookFormat) -> Result<Value, HookInstallError> {
    let Some(text) = read_optional(path)? else {
        return Ok(json!({}));
    };
    if text.trim().is_empty() {
        return Ok(json!({}));
    }
    let settings: Value = serde_json::from_str(&text).map_err(|_| invalid_config(path))?;
    if valid_shape(&settings, format) {
        Ok(settings)
    } else {
        Err(invalid_config(path))
    }
}

pub fn write_settings(path: &Path, settings: &Value) -> Result<(), HookInstallError> {
    let mut text = serde_json::to_string_pretty(settings)
        .map_err(|error| io_error(path, std::io::Error::other(error)))?;
    text.push('\n');
    write_atomic(path, &text)
}

/// Removes Lynx's handlers from every event under the root key, dropping the groups and events
/// they leave empty. Returns whether anything changed.
pub fn strip(settings: &mut Value, format: &JsonHookFormat, marker: &str) -> bool {
    let Some(root) = settings.as_object_mut() else {
        return false;
    };
    let Some(events) = root.get_mut(format.root_key).and_then(Value::as_object_mut) else {
        return false;
    };
    let changed = strip_events(events, marker);
    if changed && events.is_empty() {
        root.remove(format.root_key);
    }
    changed
}

fn strip_events(events: &mut Map<String, Value>, marker: &str) -> bool {
    let mut changed = false;
    let mut emptied = Vec::new();
    for (name, list) in events.iter_mut() {
        let Some(list) = list.as_array_mut() else {
            continue;
        };
        let mut event_changed = false;
        list.retain_mut(|item| {
            if let Some(inner) = item.get_mut("hooks").and_then(Value::as_array_mut) {
                let count = inner.len();
                inner.retain(|handler| !owned(handler, marker));
                let removed = inner.len() != count;
                event_changed |= removed;
                // A group that held only Lynx's handler goes with it; a group with others stays.
                return !(removed && inner.is_empty());
            }
            if owned(item, marker) {
                event_changed = true;
                return false;
            }
            true
        });
        if event_changed {
            changed = true;
            if list.is_empty() {
                emptied.push(name.clone());
            }
        }
    }
    for name in emptied {
        events.remove(&name);
    }
    changed
}

/// Replaces Lynx's entries with a fresh set, one per event, that runs `command`.
pub fn apply(
    settings: &mut Value,
    format: &JsonHookFormat,
    marker: &str,
    command: &str,
    events: &[EventSpec],
) {
    strip(settings, format, marker);
    let Some(root) = settings.as_object_mut() else {
        return;
    };
    if let Some(version) = format.version {
        root.entry("version").or_insert(json!(version));
    }
    let bundle = root.entry(format.root_key).or_insert_with(|| json!({}));
    let Some(bundle) = bundle.as_object_mut() else {
        return;
    };
    for spec in events {
        let base = command_for(format, command, spec.name);
        let list = bundle.entry(spec.name).or_insert_with(|| json!([]));
        if let Some(list) = list.as_array_mut() {
            list.push(entry(format, spec, &base));
        }
    }
}

/// The events whose Lynx entry is not in place, written exactly as `apply` writes it.
pub fn missing_events(
    settings: &Value,
    format: &JsonHookFormat,
    command: &str,
    events: &[EventSpec],
) -> Vec<&'static str> {
    events
        .iter()
        .filter(|spec| {
            let base = command_for(format, command, spec.name);
            let expected = entry(format, spec, &base);
            let list = settings
                .get(format.root_key)
                .and_then(|bundle| bundle.get(spec.name))
                .and_then(Value::as_array);
            !list.is_some_and(|list| list.contains(&expected))
        })
        .map(|spec| spec.name)
        .collect()
}

/// The commands one event runs in the file, for checks and tests.
#[cfg(test)]
pub fn commands_of<'a>(settings: &'a Value, format: &JsonHookFormat, event: &str) -> Vec<&'a str> {
    settings
        .get(format.root_key)
        .and_then(|bundle| bundle.get(event))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|item| match item.get("hooks").and_then(Value::as_array) {
            Some(inner) => inner.iter().collect::<Vec<_>>(),
            None => vec![item],
        })
        .filter_map(|handler| {
            ["command", "bash", "powershell"]
                .iter()
                .find_map(|key| handler.get(*key).and_then(Value::as_str))
        })
        .collect()
}

/// A settings file that Lynx writes entries into, for one harness.
#[derive(Clone)]
pub struct JsonHookTarget {
    /// The harness name the marker is built from.
    pub name: &'static str,
    pub path: PathBuf,
    pub format: JsonHookFormat,
    pub events: &'static [EventSpec],
    pub command: Option<String>,
}

impl JsonHookTarget {
    pub fn new(
        name: &'static str,
        path: PathBuf,
        format: JsonHookFormat,
        events: &'static [EventSpec],
        executable: Option<&Path>,
    ) -> Self {
        Self {
            name,
            path,
            format,
            events,
            command: executable.map(|executable| relay_command(executable, name)),
        }
    }

    fn marker(&self) -> String {
        format!(" --agent-hook {}", self.name)
    }

    pub fn status(&self) -> HookInstallStatus {
        match read_settings(&self.path, &self.format) {
            Ok(settings) => self.describe(&settings),
            Err(error) => self.status_with(HookInstallState::Error, Some(error.to_string())),
        }
    }

    /// Adds Lynx's entries, replacing any it wrote before.
    pub fn install(&self) -> Result<HookInstallStatus, HookInstallError> {
        let command = self
            .command
            .as_deref()
            .ok_or(HookInstallError::NoExecutable)?;
        let mut settings = read_settings(&self.path, &self.format)?;
        apply(
            &mut settings,
            &self.format,
            &self.marker(),
            command,
            self.events,
        );
        write_settings(&self.path, &settings)?;
        Ok(self.status())
    }

    /// Removes Lynx's entries and leaves the rest of the file as it was.
    pub fn remove(&self) -> Result<HookInstallStatus, HookInstallError> {
        let mut settings = read_settings(&self.path, &self.format)?;
        if strip(&mut settings, &self.format, &self.marker()) {
            write_settings(&self.path, &settings)?;
        }
        Ok(self.status())
    }

    fn describe(&self, settings: &Value) -> HookInstallStatus {
        let Some(command) = self.command.as_deref() else {
            return self.status_with(
                HookInstallState::Error,
                Some(HookInstallError::NoExecutable.to_string()),
            );
        };
        let missing = missing_events(settings, &self.format, command, self.events);
        let state = if missing.is_empty() {
            HookInstallState::Installed
        } else if missing.len() == self.events.len() {
            HookInstallState::NotInstalled
        } else {
            HookInstallState::Partial
        };
        let detail = (state == HookInstallState::Partial)
            .then(|| format!("Missing hooks for: {}", missing.join(", ")));
        self.status_with(state, detail)
    }

    fn status_with(&self, state: HookInstallState, detail: Option<String>) -> HookInstallStatus {
        HookInstallStatus {
            state,
            config_path: self.path.to_string_lossy().into_owned(),
            events: self
                .events
                .iter()
                .map(|spec| spec.name.to_owned())
                .collect(),
            detail,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MARKER: &str = " --agent-hook demo";
    const COMMAND: &str = "'/opt/lynx/agent-ade' --agent-hook demo";

    fn claude_handler(command: &str) -> Value {
        json!({ "type": "command", "command": command })
    }

    fn grouped_format() -> JsonHookFormat {
        JsonHookFormat {
            root_key: "hooks",
            version: None,
            event_argument: false,
            handler: claude_handler,
        }
    }

    const GROUPED: &[EventSpec] = &[
        EventSpec::grouped("Stop", None),
        EventSpec::grouped("PreToolUse", Some("*")),
    ];

    #[test]
    fn grouped_events_nest_the_handler_and_take_the_matcher() {
        let mut settings = json!({});
        apply(&mut settings, &grouped_format(), MARKER, COMMAND, GROUPED);
        assert_eq!(settings["hooks"]["Stop"][0]["hooks"][0]["command"], COMMAND);
        assert_eq!(settings["hooks"]["PreToolUse"][0]["matcher"], "*");
    }

    #[test]
    fn flat_events_list_the_handler_with_its_matcher_beside_it() {
        let format = JsonHookFormat {
            root_key: "hooks",
            version: None,
            event_argument: false,
            handler: |command| json!({ "command": command, "timeout_ms": 5000 }),
        };
        const FLAT: &[EventSpec] = &[EventSpec::flat("preToolUse", Some("*"))];
        let mut settings = json!({});
        apply(&mut settings, &format, MARKER, COMMAND, FLAT);
        let handler = &settings["hooks"]["preToolUse"][0];
        assert_eq!(handler["matcher"], "*");
        assert_eq!(handler["timeout_ms"], 5000);
    }

    #[test]
    fn a_version_is_set_only_when_the_file_has_none() {
        let format = JsonHookFormat {
            version: Some(1),
            ..grouped_format()
        };
        let mut settings = json!({});
        apply(&mut settings, &format, MARKER, COMMAND, GROUPED);
        assert_eq!(settings["version"], 1);
        let mut kept = json!({ "version": 7 });
        apply(&mut kept, &format, MARKER, COMMAND, GROUPED);
        assert_eq!(kept["version"], 7);
    }

    #[test]
    fn copilot_style_commands_carry_their_event_as_an_argument() {
        let format = JsonHookFormat {
            event_argument: true,
            ..grouped_format()
        };
        const FLAT: &[EventSpec] = &[EventSpec::flat("Stop", None)];
        let mut settings = json!({});
        apply(&mut settings, &format, MARKER, COMMAND, FLAT);
        assert_eq!(
            commands_of(&settings, &format, "Stop"),
            vec![format!("{COMMAND} Stop").as_str()]
        );
        assert!(missing_events(&settings, &format, COMMAND, FLAT).is_empty());
    }

    #[test]
    fn strip_removes_only_our_handlers_and_keeps_other_keys() {
        let mut settings = json!({
            "model": "x",
            "hooks": {
                "Stop": [
                    { "hooks": [{ "type": "command", "command": "other tool" }] },
                    { "hooks": [{ "type": "command", "command": COMMAND }] }
                ]
            }
        });
        assert!(strip(&mut settings, &grouped_format(), MARKER));
        assert_eq!(settings["model"], "x");
        assert_eq!(
            commands_of(&settings, &grouped_format(), "Stop"),
            vec!["other tool"]
        );
    }

    #[test]
    fn a_missing_event_is_named_and_a_present_one_is_not() {
        let mut settings = json!({});
        apply(
            &mut settings,
            &grouped_format(),
            MARKER,
            COMMAND,
            &GROUPED[..1],
        );
        assert_eq!(
            missing_events(&settings, &grouped_format(), COMMAND, GROUPED),
            vec!["PreToolUse"]
        );
    }

    #[test]
    fn a_file_with_another_shape_is_refused() {
        assert!(!valid_shape(&json!({ "hooks": [] }), &grouped_format()));
        assert!(valid_shape(&json!({ "hooks": {} }), &grouped_format()));
    }
}
