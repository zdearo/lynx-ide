//! The line that starts an agent in a terminal: its command, then in YOLO mode its flag that skips
//! the permission prompts, then the arguments Settings set for it. The screen shows the same flags
//! in Settings (`src/terminal/agent-presets.ts`), so a change to one needs the other.

use std::collections::BTreeMap;

use crate::agents::harness::HarnessId;
use crate::config::app_config::LaunchMode;

/// The command each agent is started with, and its YOLO flag.
fn command_of(harness: HarnessId) -> (&'static str, &'static str) {
    match harness {
        HarnessId::Claude => ("claude", "--dangerously-skip-permissions"),
        HarnessId::Codex => ("codex", "--dangerously-bypass-approvals-and-sandbox"),
        HarnessId::Opencode => ("opencode", "--auto"),
        HarnessId::Omp => ("omp", "--auto-approve"),
    }
}

/// The line typed into the shell for an agent. Arguments that are blank add nothing.
pub fn launch_line(
    harness: HarnessId,
    mode: LaunchMode,
    arguments: &BTreeMap<HarnessId, String>,
) -> String {
    let (command, yolo) = command_of(harness);
    let mut parts = vec![command];
    if mode == LaunchMode::Yolo {
        parts.push(yolo);
    }
    if let Some(extra) = arguments
        .get(&harness)
        .map(|text| text.trim())
        .filter(|text| !text.is_empty())
    {
        parts.push(extra);
    }
    parts.join(" ")
}

/// The settings an unattended agent starts with, as JSON. Nobody answers its first-run prompts, so the ones that
/// would stop it are turned off: the bypass warning and the renderer choice. Only Claude Code takes these keys.
pub fn unattended_settings(harness: HarnessId) -> Option<&'static str> {
    match harness {
        HarnessId::Claude => Some(r#"{"skipDangerousModePermissionPrompt":true,"tui":"default"}"#),
        HarnessId::Codex | HarnessId::Opencode | HarnessId::Omp => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments(pairs: &[(HarnessId, &str)]) -> BTreeMap<HarnessId, String> {
        pairs
            .iter()
            .map(|(harness, text)| (*harness, (*text).to_owned()))
            .collect()
    }

    #[test]
    fn the_command_is_followed_by_the_arguments_turned_on_for_it() {
        let set = arguments(&[(HarnessId::Claude, "--model opus")]);
        assert_eq!(
            launch_line(HarnessId::Claude, LaunchMode::Manual, &set),
            "claude --model opus"
        );
    }

    #[test]
    fn the_plain_command_when_the_arguments_are_off() {
        let other = arguments(&[(HarnessId::Codex, "--x")]);
        assert_eq!(
            launch_line(HarnessId::Claude, LaunchMode::Manual, &other),
            "claude"
        );
        assert_eq!(
            launch_line(HarnessId::Claude, LaunchMode::Manual, &BTreeMap::new()),
            "claude"
        );
    }

    #[test]
    fn an_unattended_claude_gets_settings_that_skip_its_first_run_prompts() {
        let settings = unattended_settings(HarnessId::Claude).expect("claude has settings");
        assert!(settings.contains("\"skipDangerousModePermissionPrompt\":true"));
        assert!(settings.contains("\"tui\":\"default\""));
        assert_eq!(unattended_settings(HarnessId::Codex), None);
    }

    #[test]
    fn yolo_adds_the_agents_flag_before_its_arguments() {
        let set = arguments(&[(HarnessId::Claude, "--model opus")]);
        assert_eq!(
            launch_line(HarnessId::Claude, LaunchMode::Yolo, &set),
            "claude --dangerously-skip-permissions --model opus"
        );
    }

    #[test]
    fn blank_arguments_add_nothing_but_the_yolo_flag_stays() {
        let blank = arguments(&[(HarnessId::Codex, "   ")]);
        assert_eq!(
            launch_line(HarnessId::Codex, LaunchMode::Yolo, &blank),
            "codex --dangerously-bypass-approvals-and-sandbox"
        );
        assert_eq!(
            launch_line(HarnessId::Codex, LaunchMode::Manual, &blank),
            "codex"
        );
    }

    #[test]
    fn the_arguments_are_trimmed() {
        let set = arguments(&[(HarnessId::Opencode, "  --model x  ")]);
        assert_eq!(
            launch_line(HarnessId::Opencode, LaunchMode::Manual, &set),
            "opencode --model x"
        );
    }

    /// The flags the Settings screen shows, pinned here so a change to one side fails a test.
    #[test]
    fn each_agent_has_the_yolo_flag_the_settings_screen_names() {
        assert_eq!(
            command_of(HarnessId::Claude).1,
            "--dangerously-skip-permissions"
        );
        assert_eq!(
            command_of(HarnessId::Codex).1,
            "--dangerously-bypass-approvals-and-sandbox"
        );
        assert_eq!(command_of(HarnessId::Opencode).1, "--auto");
        assert_eq!(command_of(HarnessId::Omp).1, "--auto-approve");
    }
}
