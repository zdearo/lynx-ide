//! The agent CLIs Lynx knows by name. A session started from one of them is an agent session:
//! it gets a status and shows up in the sidebar. Plain shells never do.

use std::path::Path;

use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum HarnessId {
    Claude,
    Codex,
    Opencode,
    Antigravity,
    Cursor,
    Copilot,
    Grok,
    Droid,
    Kiro,
    Qwen,
    Pi,
    Omp,
}

struct HarnessSpec {
    id: HarnessId,
    /// The name the relay and the hook commands use: `--agent-hook <name>`.
    name: &'static str,
    /// The program the user types, which is how Lynx finds the agent on the computer.
    executable: &'static str,
}

const HARNESSES: &[HarnessSpec] = &[
    HarnessSpec {
        id: HarnessId::Claude,
        name: "claude",
        executable: "claude",
    },
    HarnessSpec {
        id: HarnessId::Codex,
        name: "codex",
        executable: "codex",
    },
    HarnessSpec {
        id: HarnessId::Opencode,
        name: "opencode",
        executable: "opencode",
    },
    HarnessSpec {
        id: HarnessId::Antigravity,
        name: "antigravity",
        executable: "agy",
    },
    HarnessSpec {
        id: HarnessId::Cursor,
        name: "cursor",
        executable: "cursor-agent",
    },
    HarnessSpec {
        id: HarnessId::Copilot,
        name: "copilot",
        executable: "copilot",
    },
    HarnessSpec {
        id: HarnessId::Grok,
        name: "grok",
        executable: "grok",
    },
    HarnessSpec {
        id: HarnessId::Droid,
        name: "droid",
        executable: "droid",
    },
    HarnessSpec {
        id: HarnessId::Kiro,
        name: "kiro",
        executable: "kiro-cli",
    },
    HarnessSpec {
        id: HarnessId::Qwen,
        name: "qwen",
        executable: "qwen",
    },
    HarnessSpec {
        id: HarnessId::Pi,
        name: "pi",
        executable: "pi",
    },
    HarnessSpec {
        id: HarnessId::Omp,
        name: "omp",
        executable: "omp",
    },
];

/// The harness a typed command starts, judged by its program name: `claude`, `/usr/bin/codex`
/// and `opencode.exe` all match. Arguments after the program do not matter.
pub fn harness_for_command(command: &str) -> Option<HarnessId> {
    let program = command.split_whitespace().next()?;
    let name = Path::new(program).file_stem()?.to_str()?;
    HARNESSES
        .iter()
        .find(|spec| spec.executable.eq_ignore_ascii_case(name))
        .map(|spec| spec.id)
}

/// Every harness Lynx supports, in the order the settings list them.
pub fn supported() -> impl Iterator<Item = HarnessId> {
    HARNESSES.iter().map(|spec| spec.id)
}

/// The program a harness runs as, which is also how Lynx finds it on the computer.
pub fn executable_name(harness: HarnessId) -> &'static str {
    HARNESSES
        .iter()
        .find(|spec| spec.id == harness)
        .map_or("", |spec| spec.executable)
}

/// The name a harness goes by in hook commands and in the relay.
#[cfg(test)]
pub fn harness_name(harness: HarnessId) -> &'static str {
    HARNESSES
        .iter()
        .find(|spec| spec.id == harness)
        .map_or("", |spec| spec.name)
}

/// The harness with this name, as a relay or a hook command names it.
pub fn harness_named(name: &str) -> Option<HarnessId> {
    HARNESSES
        .iter()
        .find(|spec| spec.name.eq_ignore_ascii_case(name))
        .map(|spec| spec.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_program_name_is_recognised() {
        assert_eq!(harness_for_command("claude"), Some(HarnessId::Claude));
        assert_eq!(harness_for_command("codex"), Some(HarnessId::Codex));
        assert_eq!(harness_for_command("opencode"), Some(HarnessId::Opencode));
    }

    #[test]
    fn arguments_and_paths_do_not_hide_the_program() {
        assert_eq!(
            harness_for_command("claude --resume  abc"),
            Some(HarnessId::Claude)
        );
        assert_eq!(
            harness_for_command("/opt/bin/codex --model x"),
            Some(HarnessId::Codex)
        );
        assert_eq!(
            harness_for_command("  OpenCode.exe"),
            Some(HarnessId::Opencode)
        );
    }

    #[test]
    fn a_harness_can_be_named_directly() {
        assert_eq!(harness_named("claude"), Some(HarnessId::Claude));
        assert_eq!(harness_named("Codex"), Some(HarnessId::Codex));
        assert_eq!(harness_named("mystery"), None);
    }

    #[test]
    fn other_commands_are_not_agents() {
        assert_eq!(harness_for_command("npm test"), None);
        assert_eq!(harness_for_command("claude-tools"), None);
        assert_eq!(harness_for_command("echo claude"), None);
        assert_eq!(harness_for_command("   "), None);
    }
}
