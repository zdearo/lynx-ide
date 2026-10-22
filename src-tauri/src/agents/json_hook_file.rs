//! Hook entries in a JSON settings file, the shape most agents share. Lynx owns the entries whose
//! command carries its marker; every other entry and every other key stays as it is.

use std::path::Path;

use serde_json::{Map, Value, json};

use super::hook_install::{
    HookInstallError, invalid_config, io_error, read_optional, write_atomic,
};

/// One event Lynx listens to. Its handler sits in a group with an optional matcher.
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
}

/// The file layout of one agent: where its events live and what a handler looks like.
#[derive(Debug, Clone, Copy)]
pub struct JsonHookFormat {
    /// The key holding the events: `hooks`.
    pub root_key: &'static str,
    /// The handler object for one command.
    pub handler: fn(&str) -> Value,
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
    let bundle = root.entry(format.root_key).or_insert_with(|| json!({}));
    let Some(bundle) = bundle.as_object_mut() else {
        return;
    };
    for spec in events {
        let base = command.to_owned();
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
            let base = command.to_owned();
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

#[cfg(test)]
mod tests {
    use super::*;

    const MARKER: &str = " --agent-hook demo";
    const COMMAND: &str = "'/opt/lynx/lynx-ide' --agent-hook demo";

    fn claude_handler(command: &str) -> Value {
        json!({ "type": "command", "command": command })
    }

    fn grouped_format() -> JsonHookFormat {
        JsonHookFormat {
            root_key: "hooks",
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
