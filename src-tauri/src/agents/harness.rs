//! The agent CLIs Lynx knows by name. A session started from one of them is an agent session:
//! it gets a status and shows up in the sidebar. Plain shells never do.

use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Type,
)]
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
    harness_of_program(command.split_whitespace().next()?)
}

/// The harness a single program is, by its file name. The program may be a path with spaces in it.
fn harness_of_program(program: &str) -> Option<HarnessId> {
    let name = program_name(program)?;
    HARNESSES
        .iter()
        .find(|spec| spec.executable.eq_ignore_ascii_case(name))
        .map(|spec| spec.id)
}

/// A program's name without its folder or extension, for both `/` and `\` paths:
/// `/usr/bin/codex`, `C:\Users\Some Name\claude.exe`.
fn program_name(path: &str) -> Option<&str> {
    let file = path.rsplit(['/', '\\']).next()?;
    let name = match file.rfind('.') {
        Some(dot) if dot > 0 => &file[..dot],
        _ => file,
    };
    (!name.is_empty()).then_some(name)
}

/// Programs that run a script, so the script is what names the agent: `node .../claude`.
const RUNTIMES: &[&str] = &[
    "node", "nodejs", "bun", "deno", "python", "python3", "sh", "bash", "zsh", "dash",
];

/// The harness a running job is, from its argument list. The job's program names the agent, or
/// the script a runtime was given. Other arguments do not count, so `vim claude.md` is not an agent.
pub fn harness_of_job(argv: &[String]) -> Option<HarnessId> {
    let program = argv.first()?;
    if let Some(harness) = harness_of_program(program) {
        return Some(harness);
    }
    let name = program_name(program)?;
    if RUNTIMES
        .iter()
        .any(|runtime| runtime.eq_ignore_ascii_case(name))
    {
        return harness_of_program(argv.get(1)?);
    }
    None
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

    #[test]
    fn a_job_is_the_agent_it_runs_as_a_program_or_as_a_script() {
        let job = |args: &[&str]| args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
        assert_eq!(
            harness_of_job(&job(&["claude", "--model", "x"])),
            Some(HarnessId::Claude)
        );
        assert_eq!(
            harness_of_job(&job(&["node", "/usr/local/bin/codex"])),
            Some(HarnessId::Codex)
        );
        assert_eq!(
            harness_of_job(&job(&["/bin/sh", "/tmp/fake/claude"])),
            Some(HarnessId::Claude)
        );
    }

    #[test]
    fn a_job_that_only_mentions_an_agent_is_not_one() {
        let job = |args: &[&str]| args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
        assert_eq!(harness_of_job(&job(&["vim", "claude.md"])), None);
        assert_eq!(harness_of_job(&job(&["node", "server.js", "codex"])), None);
        assert_eq!(harness_of_job(&[]), None);
    }

    #[test]
    fn a_windows_path_with_spaces_still_names_the_agent() {
        let job = |args: &[&str]| args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
        assert_eq!(
            harness_of_job(&job(&[
                r"C:\Users\Some Name\AppData\Roaming\npm\claude.cmd",
                "--model",
                "x",
            ])),
            Some(HarnessId::Claude)
        );
        assert_eq!(
            harness_of_job(&job(&[
                r"C:\Program Files\nodejs\node.exe",
                r"C:\Users\Some Name\AppData\Roaming\npm\codex",
            ])),
            Some(HarnessId::Codex)
        );
        assert_eq!(
            harness_for_command(r"C:\Tools\opencode.exe --help"),
            Some(HarnessId::Opencode)
        );
    }
}
