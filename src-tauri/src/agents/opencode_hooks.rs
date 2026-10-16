//! OpenCode status: a plugin file in OpenCode's config folder. OpenCode loads every plugin there
//! when it starts, and the plugin hands each session event that matters to Lynx's relay. Nothing
//! needs trusting, so the file is the whole integration.

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::activity::HookEvent;
use super::plugin_file::PluginFileTarget;

/// The plugin's file name. Its first line marks it as Lynx's, so removal never touches another.
const FILE_NAME: &str = "lynx-agent-status.js";
const MARKER: &str = "// Installed by Lynx: OpenCode status plugin.";

/// The OpenCode events the plugin forwards. Everything else would cost a process per event.
const FORWARDED_EVENTS: &[&str] = &[
    "session.created",
    "session.status",
    "session.idle",
    "session.error",
    "session.deleted",
    "permission.asked",
    "permission.replied",
];

/// The event a forwarded OpenCode event stands for, or None when Lynx does not track it.
pub fn event_from_payload(payload: &Value) -> Option<HookEvent> {
    match payload.get("type")?.as_str()? {
        "session.created" => Some(HookEvent::Started),
        "session.status" => {
            let kind = payload
                .pointer("/properties/status/type")
                .and_then(Value::as_str)
                .or_else(|| {
                    payload
                        .pointer("/properties/status")
                        .and_then(Value::as_str)
                })?;
            // Idle is left to session.idle, which says when the turn finished.
            match kind {
                "busy" | "retry" | "pending" | "running" | "working" | "streaming" | "active" => {
                    Some(HookEvent::Working)
                }
                _ => None,
            }
        }
        "session.idle" | "session.error" => Some(HookEvent::Finished),
        "permission.asked" => Some(HookEvent::NeedsYou),
        "permission.replied" => Some(HookEvent::Working),
        "session.deleted" => Some(HookEvent::Ended),
        _ => None,
    }
}

/// The plugin source for this app build. The relay path is baked in, as a JSON string so any
/// character in the path is safe.
pub fn plugin_source(executable: &Path) -> String {
    let relay = serde_json::to_string(&executable.to_string_lossy()).unwrap_or_default();
    let forwarded = serde_json::to_string(FORWARDED_EVENTS).unwrap_or_default();
    format!(
        r#"{MARKER}
// Rewritten when Lynx's settings reconnect OpenCode; editing it by hand is undone that way.
import {{ spawn }} from "node:child_process";

const RELAY = {relay};
const FORWARDED = new Set({forwarded});

// Outside a Lynx terminal there is nobody to tell, so nothing is started.
function forward(event) {{
  if (!process.env.LYNX_SESSION_ID) return;
  try {{
    const child = spawn(RELAY, ["--agent-hook", "opencode"], {{
      stdio: ["pipe", "ignore", "ignore"],
    }});
    child.on("error", () => {{}});
    child.stdin.on("error", () => {{}});
    child.stdin.end(JSON.stringify(event));
  }} catch {{
    // A status report must never break the agent.
  }}
}}

export default {{
  id: "lynx.agent-status",
  server: async () => ({{
    event: async ({{ event }}) => {{
      if (FORWARDED.has(event?.type)) forward(event);
    }},
  }}),
  setup() {{}},
}};
"#
    )
}

/// OpenCode's plugin: a file in the plugins folder of its config folder.
pub fn target(config_dir: PathBuf, executable: Option<&Path>) -> PluginFileTarget {
    PluginFileTarget::new(
        config_dir.join("plugins").join(FILE_NAME),
        MARKER.to_owned(),
        executable.map(plugin_source),
        FORWARDED_EVENTS,
    )
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;

    use super::*;
    use crate::agents::hook_install::HookInstallState;

    const EXE: &str = "/opt/lynx/agent-ade";

    #[test]
    fn opencode_events_map_to_what_the_activity_tracks() {
        let event = |kind: &str| json!({ "type": kind });
        assert_eq!(
            event_from_payload(&event("session.created")),
            Some(HookEvent::Started)
        );
        assert_eq!(
            event_from_payload(&event("session.idle")),
            Some(HookEvent::Finished)
        );
        assert_eq!(
            event_from_payload(&event("session.error")),
            Some(HookEvent::Finished)
        );
        assert_eq!(
            event_from_payload(&event("permission.asked")),
            Some(HookEvent::NeedsYou)
        );
        assert_eq!(
            event_from_payload(&event("permission.replied")),
            Some(HookEvent::Working)
        );
        assert_eq!(
            event_from_payload(&event("session.deleted")),
            Some(HookEvent::Ended)
        );
        assert_eq!(event_from_payload(&event("message.part.updated")), None);
    }

    #[test]
    fn a_busy_session_is_working_and_an_idle_one_waits_for_session_idle() {
        let status = |kind: &str| json!({ "type": "session.status", "properties": { "status": { "type": kind } } });
        assert_eq!(
            event_from_payload(&status("busy")),
            Some(HookEvent::Working)
        );
        assert_eq!(event_from_payload(&status("idle")), None);
    }

    #[test]
    fn the_plugin_names_the_relay_and_the_forwarded_events() {
        let source = plugin_source(Path::new(EXE));
        assert!(source.starts_with(MARKER));
        assert!(source.contains("const RELAY = \"/opt/lynx/agent-ade\";"));
        assert!(source.contains("\"permission.asked\""));
        assert!(source.contains("--agent-hook"));
    }

    #[test]
    fn install_writes_the_plugin_and_status_sees_it() {
        let dir = tempfile::tempdir().expect("temp dir");
        let target = target(dir.path().to_path_buf(), Some(Path::new(EXE)));
        assert_eq!(target.status().state, HookInstallState::NotInstalled);

        target.install().expect("install");

        assert_eq!(target.status().state, HookInstallState::Installed);
        assert!(dir.path().join("plugins").join(FILE_NAME).exists());
    }

    #[test]
    fn a_plugin_from_another_build_is_partial_until_updated() {
        let dir = tempfile::tempdir().expect("temp dir");
        let old = target(dir.path().to_path_buf(), Some(Path::new("/old/agent-ade")));
        old.install().expect("install old");

        let current = target(dir.path().to_path_buf(), Some(Path::new(EXE)));
        assert_eq!(current.status().state, HookInstallState::Partial);
        current.install().expect("update");
        assert_eq!(current.status().state, HookInstallState::Installed);
    }

    #[test]
    fn remove_deletes_only_lynx_plugin_and_leaves_other_plugins_alone() {
        let dir = tempfile::tempdir().expect("temp dir");
        let plugins = dir.path().join("plugins");
        fs::create_dir_all(&plugins).expect("plugins dir");
        fs::write(plugins.join("herdr.js"), "// someone else\n").expect("write other");
        let target = target(dir.path().to_path_buf(), Some(Path::new(EXE)));
        target.install().expect("install");

        target.remove().expect("remove");

        assert!(!plugins.join(FILE_NAME).exists());
        assert!(plugins.join("herdr.js").exists());
    }

    #[test]
    fn install_refuses_to_overwrite_a_file_that_is_not_lynx_s() {
        let dir = tempfile::tempdir().expect("temp dir");
        let plugins = dir.path().join("plugins");
        fs::create_dir_all(&plugins).expect("plugins dir");
        fs::write(plugins.join(FILE_NAME), "// not ours\n").expect("write");
        let target = target(dir.path().to_path_buf(), Some(Path::new(EXE)));

        assert!(target.install().is_err());
        assert_eq!(
            fs::read_to_string(plugins.join(FILE_NAME)).expect("read"),
            "// not ours\n"
        );
    }
}
