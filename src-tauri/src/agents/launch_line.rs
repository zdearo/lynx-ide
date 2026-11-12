//! The line that starts an agent in a terminal: its command, then in YOLO mode its flag that skips
//! the permission prompts, then the arguments Settings set for it. The screen shows the same flags
//! in Settings (`src/terminal/agent-presets.ts`), so a change to one needs the other.

use std::collections::BTreeMap;

use crate::agents::harness::HarnessId;
use crate::config::app_config::{AgentEffort, LaunchMode};

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
    launch_line_with_effort(harness, mode, arguments, None)
}

/// The same line with the effort a worker asks for. `None` adds no effort flag, so the agent keeps its
/// own. The effort flag goes after the YOLO flag and before the typed arguments, so an argument the
/// user writes by hand still comes last.
pub fn launch_line_with_effort(
    harness: HarnessId,
    mode: LaunchMode,
    arguments: &BTreeMap<HarnessId, String>,
    effort: Option<AgentEffort>,
) -> String {
    let (command, yolo) = command_of(harness);
    let mut parts = vec![command.to_owned()];
    if mode == LaunchMode::Yolo {
        parts.push(yolo.to_owned());
    }
    if let Some(flag) = effort.and_then(|effort| effort_flag(harness, effort)) {
        parts.push(flag);
    }
    if let Some(extra) = arguments
        .get(&harness)
        .map(|text| text.trim())
        .filter(|text| !text.is_empty())
    {
        parts.push(extra.to_owned());
    }
    parts.join(" ")
}

/// The flag that sets an agent's effort, or `None` for an agent without one. Claude takes the level by
/// name; Codex takes it as a config value.
fn effort_flag(harness: HarnessId, effort: AgentEffort) -> Option<String> {
    match harness {
        HarnessId::Claude => Some(format!("--effort {}", effort_name(effort))),
        HarnessId::Codex => {
            // shortcut: Codex's build is checked only up to xhigh, so max is sent as xhigh, send max once its acceptance is tested.
            let level = match effort {
                AgentEffort::Max => AgentEffort::Xhigh,
                other => other,
            };
            Some(format!("-c model_reasoning_effort={}", effort_name(level)))
        }
        HarnessId::Opencode | HarnessId::Omp => None,
    }
}

fn effort_name(effort: AgentEffort) -> &'static str {
    match effort {
        AgentEffort::Low => "low",
        AgentEffort::Medium => "medium",
        AgentEffort::High => "high",
        AgentEffort::Xhigh => "xhigh",
        AgentEffort::Max => "max",
    }
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

    const LEVELS: [AgentEffort; 5] = [
        AgentEffort::Low,
        AgentEffort::Medium,
        AgentEffort::High,
        AgentEffort::Xhigh,
        AgentEffort::Max,
    ];

    #[test]
    fn claude_takes_each_effort_level_by_name_in_its_effort_flag() {
        let none = BTreeMap::new();
        let lines: Vec<String> = LEVELS
            .iter()
            .map(|level| {
                launch_line_with_effort(HarnessId::Claude, LaunchMode::Manual, &none, Some(*level))
            })
            .collect();
        assert_eq!(
            lines,
            [
                "claude --effort low",
                "claude --effort medium",
                "claude --effort high",
                "claude --effort xhigh",
                "claude --effort max",
            ]
        );
    }

    #[test]
    fn codex_takes_its_reasoning_effort_as_a_config_value() {
        let none = BTreeMap::new();
        let line = |level| {
            launch_line_with_effort(HarnessId::Codex, LaunchMode::Manual, &none, Some(level))
        };
        assert_eq!(
            line(AgentEffort::Low),
            "codex -c model_reasoning_effort=low"
        );
        assert_eq!(
            line(AgentEffort::Medium),
            "codex -c model_reasoning_effort=medium"
        );
        assert_eq!(
            line(AgentEffort::High),
            "codex -c model_reasoning_effort=high"
        );
        assert_eq!(
            line(AgentEffort::Xhigh),
            "codex -c model_reasoning_effort=xhigh"
        );
    }

    #[test]
    fn codex_gets_max_as_xhigh_until_max_is_checked_on_its_build() {
        let none = BTreeMap::new();
        assert_eq!(
            launch_line_with_effort(
                HarnessId::Codex,
                LaunchMode::Manual,
                &none,
                Some(AgentEffort::Max)
            ),
            "codex -c model_reasoning_effort=xhigh"
        );
    }

    #[test]
    fn opencode_and_omp_ignore_the_effort() {
        let none = BTreeMap::new();
        for level in LEVELS {
            assert_eq!(
                launch_line_with_effort(
                    HarnessId::Opencode,
                    LaunchMode::Manual,
                    &none,
                    Some(level)
                ),
                "opencode"
            );
            assert_eq!(
                launch_line_with_effort(HarnessId::Omp, LaunchMode::Manual, &none, Some(level)),
                "omp"
            );
        }
    }

    #[test]
    fn no_effort_adds_no_flag_and_matches_the_plain_line() {
        let set = arguments(&[(HarnessId::Claude, "--model opus")]);
        for harness in [
            HarnessId::Claude,
            HarnessId::Codex,
            HarnessId::Opencode,
            HarnessId::Omp,
        ] {
            assert_eq!(
                launch_line_with_effort(harness, LaunchMode::Yolo, &set, None),
                launch_line(harness, LaunchMode::Yolo, &set)
            );
        }
    }

    #[test]
    fn the_effort_flag_sits_after_yolo_and_before_the_typed_arguments() {
        let set = arguments(&[(HarnessId::Claude, "--model opus")]);
        assert_eq!(
            launch_line_with_effort(
                HarnessId::Claude,
                LaunchMode::Yolo,
                &set,
                Some(AgentEffort::Low)
            ),
            "claude --dangerously-skip-permissions --effort low --model opus"
        );
        let codex = arguments(&[(HarnessId::Codex, "--search")]);
        assert_eq!(
            launch_line_with_effort(
                HarnessId::Codex,
                LaunchMode::Yolo,
                &codex,
                Some(AgentEffort::High)
            ),
            "codex --dangerously-bypass-approvals-and-sandbox -c model_reasoning_effort=high --search"
        );
    }
}
