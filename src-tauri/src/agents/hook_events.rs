//! The event a hook payload stands for, for each harness. Claude, Codex and OpenCode have their own
//! modules, since their payloads need more than a name; OMP is read here by event name.

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
        HarnessId::Claude => claude_hooks::event_from_payload(payload),
        HarnessId::Codex => codex_hooks::event_from_payload(payload),
        HarnessId::Opencode => opencode_hooks::event_from_payload(payload),
        HarnessId::Omp => omp_event(payload),
    }
}

/// The agent's name for its own conversation, when a hook carries it. It is typed into a shell to
/// resume the conversation later, so only plain names pass.
pub fn resume_id_of(harness: HarnessId, payload: &Value) -> Option<String> {
    let id = match harness {
        HarnessId::Claude | HarnessId::Codex => payload.get("session_id")?.as_str()?,
        HarnessId::Opencode => payload.pointer("/properties/info/id")?.as_str()?,
        HarnessId::Omp => return None,
    };
    let plain = !id.is_empty()
        && id.len() <= 128
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    plain.then(|| id.to_owned())
}

fn event_name(payload: &Value) -> Option<&str> {
    payload.get(EVENT_KEY)?.as_str()
}

fn omp_event(payload: &Value) -> Option<HookEvent> {
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
    fn omp_reports_the_extension_events() {
        assert_eq!(
            event_for(HarnessId::Omp, &named("tool_approval_requested")),
            Some(HookEvent::NeedsYou)
        );
        assert_eq!(
            event_for(HarnessId::Omp, &named("session_shutdown")),
            Some(HookEvent::Ended)
        );
        assert_eq!(
            event_for(HarnessId::Omp, &named("tool_execution_end")),
            None
        );
    }

    #[test]
    fn a_resume_name_comes_from_the_hooks_that_carry_it_and_must_be_plain() {
        let claude = json!({ "session_id": "abc-123_x", "hook_event_name": "SessionStart" });
        assert_eq!(
            resume_id_of(HarnessId::Claude, &claude).as_deref(),
            Some("abc-123_x")
        );
        assert_eq!(
            resume_id_of(HarnessId::Claude, &json!({ "session_id": "a; rm -rf ~" })),
            None,
            "a name that is not plain is never typed into a shell"
        );
        let opencode = json!({ "properties": { "info": { "id": "ses_42" } } });
        assert_eq!(
            resume_id_of(HarnessId::Opencode, &opencode).as_deref(),
            Some("ses_42")
        );
        assert_eq!(resume_id_of(HarnessId::Omp, &claude), None);
    }
}
