//! The event a hook payload stands for, for each harness. Claude, Codex and OpenCode have their own
//! modules, since their payloads need more than a name; the rest are read here by event name.

use serde_json::Value;

use super::activity::HookEvent;
use super::claude_hooks;
use super::codex_hooks;
use super::harness::HarnessId;
use super::opencode_hooks;

/// The key that names the event in a payload. The relay fills it in from the hook's argument when
/// a harness does not put the name in its payload.
pub const EVENT_KEY: &str = "hook_event_name";

pub fn event_for(harness: HarnessId, payload: &Value) -> Option<HookEvent> {
    match harness {
        // Qwen and Droid write their hooks in Claude's format.
        HarnessId::Claude | HarnessId::Qwen | HarnessId::Droid => {
            claude_hooks::event_from_payload(payload)
        }
        HarnessId::Codex => codex_hooks::event_from_payload(payload),
        HarnessId::Opencode => opencode_hooks::event_from_payload(payload),
        HarnessId::Grok => grok_event(payload),
        HarnessId::Antigravity => antigravity_event(payload),
        HarnessId::Cursor => cursor_event(payload),
        HarnessId::Copilot => copilot_event(payload),
        HarnessId::Kiro => kiro_event(payload),
        HarnessId::Pi | HarnessId::Omp => pi_event(payload),
    }
}

fn event_name(payload: &Value) -> Option<&str> {
    payload.get(EVENT_KEY)?.as_str()
}

fn grok_event(payload: &Value) -> Option<HookEvent> {
    match event_name(payload)? {
        "SessionStart" => Some(HookEvent::Started),
        "PreToolUse"
            if payload.get("tool_name").and_then(Value::as_str) == Some("ask_user_question") =>
        {
            Some(HookEvent::NeedsYou)
        }
        "UserPromptSubmit" | "PreToolUse" | "PostToolUse" | "PostToolUseFailure" => {
            Some(HookEvent::Working)
        }
        "Stop" | "StopCancelled" | "StopFailure" => Some(HookEvent::Finished),
        "SessionEnd" => Some(HookEvent::Ended),
        _ => None,
    }
}

fn antigravity_event(payload: &Value) -> Option<HookEvent> {
    match event_name(payload)? {
        "PreInvocation" | "PreToolUse" | "PostToolUse" => Some(HookEvent::Working),
        "Stop" => Some(HookEvent::Finished),
        _ => None,
    }
}

fn cursor_event(payload: &Value) -> Option<HookEvent> {
    match event_name(payload)? {
        "beforeSubmitPrompt"
        | "preToolUse"
        | "postToolUse"
        | "postToolUseFailure"
        | "beforeShellExecution"
        | "beforeMCPExecution" => Some(HookEvent::Working),
        "stop" => Some(HookEvent::Finished),
        _ => None,
    }
}

fn copilot_event(payload: &Value) -> Option<HookEvent> {
    match event_name(payload)? {
        "SessionStart" => Some(HookEvent::Started),
        "SessionEnd" => Some(HookEvent::Ended),
        "UserPromptSubmit" | "PreToolUse" | "PostToolUse" | "PostToolUseFailure" => {
            Some(HookEvent::Working)
        }
        "PermissionRequest" => Some(HookEvent::NeedsYou),
        "Stop" | "ErrorOccurred" => Some(HookEvent::Finished),
        _ => None,
    }
}

fn kiro_event(payload: &Value) -> Option<HookEvent> {
    match event_name(payload)? {
        "agentSpawn" => Some(HookEvent::Started),
        "userPromptSubmit" | "preToolUse" | "postToolUse" => Some(HookEvent::Working),
        "stop" => Some(HookEvent::Finished),
        _ => None,
    }
}

fn pi_event(payload: &Value) -> Option<HookEvent> {
    match event_name(payload)? {
        "session_start" => Some(HookEvent::Started),
        "agent_start" | "tool_call" | "tool_execution_start" | "tool_approval_resolved" => {
            Some(HookEvent::Working)
        }
        "tool_approval_requested" => Some(HookEvent::NeedsYou),
        "agent_end" | "agent_settled" => Some(HookEvent::Finished),
        "session_shutdown" => Some(HookEvent::Ended),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn named(name: &str) -> Value {
        json!({ EVENT_KEY: name })
    }

    #[test]
    fn grok_reports_a_question_as_needing_the_user() {
        let question = json!({ EVENT_KEY: "PreToolUse", "tool_name": "ask_user_question" });
        assert_eq!(
            event_for(HarnessId::Grok, &question),
            Some(HookEvent::NeedsYou)
        );
        assert_eq!(
            event_for(HarnessId::Grok, &named("StopFailure")),
            Some(HookEvent::Finished)
        );
        assert_eq!(
            event_for(HarnessId::Grok, &named("SessionEnd")),
            Some(HookEvent::Ended)
        );
    }

    #[test]
    fn antigravity_work_is_tool_and_invocation_work_and_post_invocation_is_ignored() {
        assert_eq!(
            event_for(HarnessId::Antigravity, &named("PreInvocation")),
            Some(HookEvent::Working)
        );
        assert_eq!(
            event_for(HarnessId::Antigravity, &named("Stop")),
            Some(HookEvent::Finished)
        );
        assert_eq!(
            event_for(HarnessId::Antigravity, &named("PostInvocation")),
            None
        );
    }

    #[test]
    fn cursor_and_copilot_map_their_own_event_names() {
        assert_eq!(
            event_for(HarnessId::Cursor, &named("beforeShellExecution")),
            Some(HookEvent::Working)
        );
        assert_eq!(
            event_for(HarnessId::Cursor, &named("stop")),
            Some(HookEvent::Finished)
        );
        assert_eq!(
            event_for(HarnessId::Copilot, &named("PermissionRequest")),
            Some(HookEvent::NeedsYou)
        );
        assert_eq!(
            event_for(HarnessId::Copilot, &named("ErrorOccurred")),
            Some(HookEvent::Finished)
        );
    }

    #[test]
    fn kiro_starts_and_finishes_a_session_but_has_no_end_event() {
        assert_eq!(
            event_for(HarnessId::Kiro, &named("agentSpawn")),
            Some(HookEvent::Started)
        );
        assert_eq!(
            event_for(HarnessId::Kiro, &named("preToolUse")),
            Some(HookEvent::Working)
        );
        assert_eq!(
            event_for(HarnessId::Kiro, &named("stop")),
            Some(HookEvent::Finished)
        );
    }

    #[test]
    fn pi_and_omp_share_the_extension_events() {
        for harness in [HarnessId::Pi, HarnessId::Omp] {
            assert_eq!(
                event_for(harness, &named("tool_approval_requested")),
                Some(HookEvent::NeedsYou)
            );
            assert_eq!(
                event_for(harness, &named("session_shutdown")),
                Some(HookEvent::Ended)
            );
            assert_eq!(event_for(harness, &named("tool_execution_end")), None);
        }
    }

    #[test]
    fn qwen_and_droid_read_claude_style_payloads() {
        let start = json!({ EVENT_KEY: "SessionStart", "source": "startup" });
        assert_eq!(event_for(HarnessId::Qwen, &start), Some(HookEvent::Started));
        assert_eq!(
            event_for(HarnessId::Droid, &named("PermissionRequest")),
            Some(HookEvent::NeedsYou)
        );
    }
}
