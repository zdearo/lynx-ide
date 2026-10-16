//! The answer a hook prints for its harness to read. Most harnesses take `{}` as "carry on". A few
//! need an explicit answer, because their silence means something else.

use serde_json::Value;

use super::harness::HarnessId;
use super::hook_events::EVENT_KEY;

/// The stdout for this event. Lynx only watches, so it never allows or denies a tool itself.
pub fn answer(harness: HarnessId, payload: &Value) -> &'static str {
    let event = payload
        .get(EVENT_KEY)
        .and_then(Value::as_str)
        .unwrap_or_default();
    match (harness, event) {
        // Antigravity treats a tool it got no answer for as denied; asking hands it to the user.
        (HarnessId::Antigravity, "PreToolUse") => r#"{"decision":"ask"}"#,
        (HarnessId::Cursor, "beforeSubmitPrompt") => r#"{"continue":true}"#,
        (HarnessId::Cursor, "preToolUse" | "beforeShellExecution" | "beforeMCPExecution") => {
            r#"{"permission":"ask"}"#
        }
        _ => "{}",
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn antigravity_asks_before_a_tool_runs() {
        let payload = json!({ EVENT_KEY: "PreToolUse" });
        assert_eq!(
            answer(HarnessId::Antigravity, &payload),
            r#"{"decision":"ask"}"#
        );
    }

    #[test]
    fn cursor_lets_a_prompt_through_and_asks_before_a_tool_runs() {
        let prompt = json!({ EVENT_KEY: "beforeSubmitPrompt" });
        let shell = json!({ EVENT_KEY: "beforeShellExecution" });
        assert_eq!(answer(HarnessId::Cursor, &prompt), r#"{"continue":true}"#);
        assert_eq!(answer(HarnessId::Cursor, &shell), r#"{"permission":"ask"}"#);
    }

    #[test]
    fn every_other_event_carries_on_with_an_empty_answer() {
        let stop = json!({ EVENT_KEY: "Stop" });
        assert_eq!(answer(HarnessId::Claude, &stop), "{}");
        assert_eq!(
            answer(HarnessId::Antigravity, &json!({ EVENT_KEY: "Stop" })),
            "{}"
        );
        assert_eq!(answer(HarnessId::Cursor, &json!({})), "{}");
    }
}
