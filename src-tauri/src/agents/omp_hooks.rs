//! OMP status: an extension file in OMP's own folder. OMP loads every extension there when it
//! starts, and the extension hands each session event that matters to Lynx's relay.

use std::path::{Path, PathBuf};

use super::plugin_file::PluginFileTarget;

const FILE_NAME: &str = "lynx-agent-status.ts";
const HARNESS: &str = "omp";

/// The events the extension forwards. Tool output and other events would cost a process each.
const FORWARDED_EVENTS: &[&str] = &[
    "session_start",
    "agent_start",
    "agent_end",
    "agent_settled",
    "session_shutdown",
    "tool_call",
    "tool_execution_start",
    "tool_execution_end",
    "tool_approval_requested",
    "tool_approval_resolved",
];

/// The extension source for this app build and this agent. The relay path and the harness name are
/// JSON strings, so any character in the path is safe.
pub fn extension_source(executable: &Path, harness: &str) -> String {
    let relay = serde_json::to_string(&executable.to_string_lossy()).unwrap_or_default();
    let forwarded = serde_json::to_string(FORWARDED_EVENTS).unwrap_or_default();
    format!(
        r#"{marker}
// Rewritten when Lynx's settings reconnect the agent; editing it by hand is undone that way.
import {{ spawn }} from "node:child_process";

const RELAY = {relay};
const HARNESS = "{harness}";
const FORWARDED = {forwarded};

// Outside a Lynx terminal there is nobody to tell, so nothing is started.
function forward(name) {{
  if (!process.env.LYNX_SESSION_ID) return;
  try {{
    const child = spawn(RELAY, ["--agent-hook", HARNESS], {{
      stdio: ["pipe", "ignore", "ignore"],
    }});
    child.on("error", () => {{}});
    child.stdin.on("error", () => {{}});
    child.stdin.end(JSON.stringify({{ hook_event_name: name }}));
  }} catch {{
    // A status report must never break the agent.
  }}
}}

export default function (pi) {{
  for (const name of FORWARDED) {{
    pi.on(name, () => forward(name));
  }}
}}
"#,
        marker = marker(harness),
    )
}

fn marker(harness: &str) -> String {
    format!("// Installed by Lynx: {harness} status extension.")
}

/// The extension, written into OMP's extensions folder.
pub fn target(agent_dir: PathBuf, executable: Option<&Path>) -> PluginFileTarget {
    PluginFileTarget::new(
        agent_dir.join("extensions").join(FILE_NAME),
        marker(HARNESS),
        executable.map(|executable| extension_source(executable, HARNESS)),
        FORWARDED_EVENTS,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::hook_install::HookInstallState;

    const EXE: &str = "/opt/lynx/lynx-ide";

    #[test]
    fn the_extension_names_the_relay_the_harness_and_the_events() {
        let source = extension_source(Path::new(EXE), "omp");
        assert!(source.starts_with(&marker("omp")));
        assert!(source.contains("const RELAY = \"/opt/lynx/lynx-ide\";"));
        assert!(source.contains("const HARNESS = \"omp\";"));
        assert!(source.contains("\"tool_approval_requested\""));
    }

    #[test]
    fn the_extension_goes_in_omps_own_folder() {
        let omp = target(PathBuf::from("/home/me/.omp/agent"), Some(Path::new(EXE)));
        // Compared by path component, because Windows writes the separators as backslashes.
        assert!(
            Path::new(&omp.status().config_path)
                .ends_with(".omp/agent/extensions/lynx-agent-status.ts")
        );
    }

    #[test]
    fn install_and_remove_leave_other_extensions_alone() {
        let dir = tempfile::tempdir().expect("temp dir");
        let extensions = dir.path().join("extensions");
        std::fs::create_dir_all(&extensions).expect("extensions dir");
        std::fs::write(extensions.join("other.ts"), "// someone else\n").expect("write");
        let target = target(dir.path().to_path_buf(), Some(Path::new(EXE)));
        assert_eq!(target.status().state, HookInstallState::NotInstalled);

        target.install().expect("install");
        assert_eq!(target.status().state, HookInstallState::Installed);

        target.remove().expect("remove");
        assert_eq!(target.status().state, HookInstallState::NotInstalled);
        assert!(extensions.join("other.ts").exists());
    }
}
