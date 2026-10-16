//! Where each agent that keeps its hooks in a JSON file keeps them, and which events Lynx hooks.
//! The layouts follow each agent's documentation. `json_hook_file` refuses a file of another shape,
//! so a layout that is wrong for one agent leaves that file alone rather than damaging it.

use std::path::Path;

use serde_json::{Value, json};

use super::json_hook_file::{EventSpec, JsonHookFormat, JsonHookTarget};

/// Claude's layout: a `hooks` object of event groups, each holding handlers with a `command`.
fn command_handler(command: &str) -> Value {
    json!({ "type": "command", "command": command })
}

/// Cursor: a flat list per event, with a timeout in seconds.
fn cursor_handler(command: &str) -> Value {
    json!({ "command": command, "timeout": 5 })
}

/// Copilot: a flat list per event, with the command under `bash` and a timeout in seconds.
fn copilot_handler(command: &str) -> Value {
    json!({ "type": "command", "bash": command, "timeoutSec": 5 })
}

/// Antigravity: a flat list per event, or a group with a matcher for the tool events.
fn antigravity_handler(command: &str) -> Value {
    json!({ "type": "command", "command": command, "timeout": 5 })
}

/// The events Claude-style agents report, with the matcher each one needs.
const CLAUDE_STYLE_EVENTS: &[EventSpec] = &[
    EventSpec::grouped("SessionStart", None),
    EventSpec::grouped("SessionEnd", None),
    EventSpec::grouped("UserPromptSubmit", None),
    EventSpec::grouped("PreToolUse", Some("*")),
    EventSpec::grouped("PostToolUse", Some("*")),
    EventSpec::grouped("PostToolUseFailure", Some("*")),
    EventSpec::grouped("PermissionRequest", Some("*")),
    EventSpec::grouped("Stop", None),
    EventSpec::grouped("StopFailure", None),
];

const CLAUDE_STYLE: JsonHookFormat = JsonHookFormat {
    root_key: "hooks",
    version: None,
    event_argument: false,
    handler: command_handler,
};

/// Qwen Code's settings file, which holds the Claude-style hooks next to its own settings.
pub fn qwen(user_home: &Path, executable: Option<&Path>) -> JsonHookTarget {
    JsonHookTarget::new(
        "qwen",
        user_home.join(".qwen").join("settings.json"),
        CLAUDE_STYLE,
        CLAUDE_STYLE_EVENTS,
        executable,
    )
}

/// Droid's settings file. Droid reports a session's start and each turn, but not its end.
pub fn droid(user_home: &Path, executable: Option<&Path>) -> JsonHookTarget {
    const EVENTS: &[EventSpec] = &[
        EventSpec::grouped("SessionStart", None),
        EventSpec::grouped("UserPromptSubmit", None),
        EventSpec::grouped("PreToolUse", Some("*")),
        EventSpec::grouped("PostToolUse", Some("*")),
        EventSpec::grouped("PermissionRequest", Some("*")),
        EventSpec::grouped("Stop", None),
    ];
    JsonHookTarget::new(
        "droid",
        user_home.join(".factory").join("settings.json"),
        CLAUDE_STYLE,
        EVENTS,
        executable,
    )
}

/// Grok reads its hooks from a folder, where Lynx keeps its own file. Its command gets the event
/// as an argument, so each event runs the same relay under its own name.
pub fn grok(grok_home: &Path, executable: Option<&Path>) -> JsonHookTarget {
    const EVENTS: &[EventSpec] = &[
        EventSpec::grouped("SessionStart", None),
        EventSpec::grouped("UserPromptSubmit", None),
        EventSpec::grouped("PreToolUse", Some(".*")),
        EventSpec::grouped("PostToolUse", Some(".*")),
        EventSpec::grouped("PostToolUseFailure", Some(".*")),
        EventSpec::grouped("Stop", None),
        EventSpec::grouped("StopCancelled", None),
        EventSpec::grouped("StopFailure", None),
        EventSpec::grouped("SessionEnd", None),
    ];
    let format = JsonHookFormat {
        event_argument: true,
        ..CLAUDE_STYLE
    };
    JsonHookTarget::new(
        "grok",
        grok_home.join("hooks").join("lynx-status.json"),
        format,
        EVENTS,
        executable,
    )
}

/// Cursor's hooks file, which Cursor reads next to any hooks the user wrote.
pub fn cursor(user_home: &Path, executable: Option<&Path>) -> JsonHookTarget {
    const EVENTS: &[EventSpec] = &[
        EventSpec::flat("beforeSubmitPrompt", None),
        EventSpec::flat("preToolUse", None),
        EventSpec::flat("postToolUse", None),
        EventSpec::flat("postToolUseFailure", None),
        EventSpec::flat("beforeShellExecution", None),
        EventSpec::flat("beforeMCPExecution", None),
        EventSpec::flat("stop", None),
    ];
    let format = JsonHookFormat {
        root_key: "hooks",
        version: Some(1),
        event_argument: true,
        handler: cursor_handler,
    };
    JsonHookTarget::new(
        "cursor",
        user_home.join(".cursor").join("hooks.json"),
        format,
        EVENTS,
        executable,
    )
}

/// Copilot's hooks folder: Lynx keeps its own file there.
pub fn copilot(copilot_home: &Path, executable: Option<&Path>) -> JsonHookTarget {
    const EVENTS: &[EventSpec] = &[
        EventSpec::flat("SessionStart", None),
        EventSpec::flat("SessionEnd", None),
        EventSpec::flat("UserPromptSubmit", None),
        EventSpec::flat("PreToolUse", None),
        EventSpec::flat("PostToolUse", None),
        EventSpec::flat("PostToolUseFailure", None),
        EventSpec::flat("PermissionRequest", None),
        EventSpec::flat("Stop", None),
        EventSpec::flat("ErrorOccurred", None),
    ];
    let format = JsonHookFormat {
        root_key: "hooks",
        version: Some(1),
        event_argument: true,
        handler: copilot_handler,
    };
    JsonHookTarget::new(
        "copilot",
        copilot_home.join("hooks").join("lynx.json"),
        format,
        EVENTS,
        executable,
    )
}

/// Antigravity's shared hooks file. Lynx's entries sit in their own bundle, so the user's bundles
/// stay as they are.
pub fn antigravity(user_home: &Path, executable: Option<&Path>) -> JsonHookTarget {
    const EVENTS: &[EventSpec] = &[
        EventSpec::flat("PreInvocation", None),
        EventSpec::flat("PostInvocation", None),
        EventSpec::flat("Stop", None),
        EventSpec::grouped("PreToolUse", Some("*")),
        EventSpec::grouped("PostToolUse", Some("*")),
    ];
    let format = JsonHookFormat {
        root_key: "lynx-status",
        version: None,
        event_argument: true,
        handler: antigravity_handler,
    };
    JsonHookTarget::new(
        "antigravity",
        user_home.join(".gemini").join("config").join("hooks.json"),
        format,
        EVENTS,
        executable,
    )
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;

    use super::*;
    use crate::agents::hook_install::HookInstallState;

    const EXE: &str = "/opt/lynx/lynx-ide";

    /// Every layout writes its entries, reports them, and takes them back out.
    #[test]
    fn each_layout_installs_reports_and_removes_its_entries() {
        let home = tempfile::tempdir().expect("temp dir");
        let exe = Some(Path::new(EXE));
        let targets = [
            qwen(home.path(), exe),
            droid(home.path(), exe),
            grok(&home.path().join("grok"), exe),
            cursor(home.path(), exe),
            copilot(&home.path().join("copilot"), exe),
            antigravity(home.path(), exe),
        ];
        for target in &targets {
            assert_eq!(
                target.status().state,
                HookInstallState::NotInstalled,
                "{}",
                target.name
            );
            target.install().expect("install");
            assert_eq!(
                target.status().state,
                HookInstallState::Installed,
                "{}",
                target.name
            );
            target.remove().expect("remove");
            assert_eq!(
                target.status().state,
                HookInstallState::NotInstalled,
                "{}",
                target.name
            );
        }
    }

    /// A settings file Lynx shares with the user keeps every key that was not Lynx's.
    #[test]
    fn a_shared_settings_file_keeps_the_users_own_keys() {
        let home = tempfile::tempdir().expect("temp dir");
        let settings = home.path().join(".qwen").join("settings.json");
        fs::create_dir_all(settings.parent().expect("parent")).expect("folder");
        fs::write(&settings, r#"{"theme":"dark"}"#).expect("write");
        let target = qwen(home.path(), Some(Path::new(EXE)));

        target.install().expect("install");
        target.remove().expect("remove");

        let kept: Value =
            serde_json::from_str(&fs::read_to_string(&settings).expect("read")).expect("json");
        assert_eq!(kept, json!({ "theme": "dark" }));
    }

    #[test]
    fn antigravity_keeps_its_entries_in_a_bundle_of_their_own() {
        let home = tempfile::tempdir().expect("temp dir");
        let target = antigravity(home.path(), Some(Path::new(EXE)));

        target.install().expect("install");

        let file = home
            .path()
            .join(".gemini")
            .join("config")
            .join("hooks.json");
        let written: Value =
            serde_json::from_str(&fs::read_to_string(file).expect("read")).expect("json");
        assert!(written["lynx-status"]["PreToolUse"].is_array());
        assert!(written.get("hooks").is_none());
    }
}
