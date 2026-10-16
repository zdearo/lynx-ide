//! `agent-ade --agent-hook <harness> [event]`: what a harness runs for each hook event. It hands the
//! event to the running app over loopback, then prints the harness's answer, which is `{}` for most.

use std::env;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::Duration;

use serde_json::{Value, json};

use super::harness::harness_named;
use super::hook_decision;
use super::hook_events::EVENT_KEY;
use super::hook_hub::{HOOK_PATH, PORT_ENV, SESSION_ENV, TOKEN_ENV, TOKEN_HEADER};

const FLAG: &str = "--agent-hook";
const TIMEOUT: Duration = Duration::from_secs(1);
/// The same limit the listener applies, so nothing larger is sent in the first place.
const MAX_PAYLOAD_BYTES: u64 = 1024 * 1024;

/// Runs the relay when the arguments ask for it and returns the exit code. None means the app
/// should start normally.
pub fn run_if_requested(arguments: &[String]) -> Option<i32> {
    if arguments.get(1).map(String::as_str) != Some(FLAG) {
        return None;
    }
    let mut raw = String::new();
    let _ = std::io::stdin()
        .lock()
        .take(MAX_PAYLOAD_BYTES)
        .read_to_string(&mut raw);
    let name = arguments.get(2).map_or("", String::as_str);
    let event = arguments.get(3).map(String::as_str);
    let answer = match (harness_named(name), relay_payload(&raw, event)) {
        (Some(harness), Some(payload)) => {
            let answer = hook_decision::answer(harness, &payload);
            if let Some(target) = Target::from_env() {
                forward(&target, name, payload);
            }
            answer
        }
        _ => "{}",
    };
    // The harness reads stdout as the hook's answer. Lynx never blocks anything, so it is mostly `{}`.
    println!("{answer}");
    Some(0)
}

/// The payload the app receives: the harness's JSON, with the event from the command's argument
/// filled in when the payload does not name it. Input that is neither JSON nor a named event is
/// dropped.
fn relay_payload(raw: &str, event: Option<&str>) -> Option<Value> {
    let mut payload = match serde_json::from_str::<Value>(raw) {
        Ok(value @ Value::Object(_)) => value,
        _ if event.is_some() => json!({}),
        _ => return None,
    };
    if let (Some(event), Some(object)) = (event, payload.as_object_mut())
        && !object.contains_key(EVENT_KEY)
    {
        object.insert(EVENT_KEY.to_owned(), json!(event));
    }
    Some(payload)
}

/// Where the relay posts: the session it runs in and the listener the app opened for it.
struct Target {
    session: String,
    port: u16,
    token: String,
}

impl Target {
    /// None outside a Lynx session, or when the app did not open its listener.
    fn from_env() -> Option<Self> {
        Some(Self {
            session: env::var(SESSION_ENV).ok()?,
            port: env::var(PORT_ENV).ok()?.parse().ok()?,
            token: env::var(TOKEN_ENV).ok()?,
        })
    }
}

/// Sends the event to the app. A hook must never break the agent, so every failure is dropped
/// without a word: the event is lost, but the agent carries on.
fn forward(target: &Target, harness: &str, payload: Value) {
    let body = json!({
        "sessionId": target.session,
        "harness": harness,
        "payload": payload,
    })
    .to_string();
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, target.port));
    let Ok(mut stream) = TcpStream::connect_timeout(&address, TIMEOUT) else {
        return;
    };
    let _ = stream.set_write_timeout(Some(TIMEOUT));
    let request = format!(
        "POST {HOOK_PATH} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\n{TOKEN_HEADER}: {token}\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n{body}",
        token = target.token,
        length = body.len(),
    );
    let _ = stream.write_all(request.as_bytes());
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;
    use crate::agents::agent_monitor::AgentInput;
    use crate::agents::harness::HarnessId;
    use crate::agents::hook_hub::HookHub;

    /// The relay's request must be the one the listener reads: same path, token and envelope.
    #[test]
    fn a_relayed_event_reaches_the_session_it_came_from() {
        let hub = HookHub::start();
        let (input, inbox) = mpsc::channel();
        hub.register("s1", input);
        let env = hub.session_env("s1");
        let value = |key: &str| {
            env.iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.clone())
                .expect("variable is set")
        };
        let target = Target {
            session: "s1".to_owned(),
            port: value(PORT_ENV).parse().expect("port"),
            token: value(TOKEN_ENV),
        };

        forward(
            &target,
            "claude",
            json!({ "hook_event_name": "UserPromptSubmit" }),
        );

        match inbox.recv_timeout(Duration::from_secs(2)) {
            Ok(AgentInput::Hook { harness, payload }) => {
                assert_eq!(harness, HarnessId::Claude);
                assert_eq!(payload["hook_event_name"], "UserPromptSubmit");
            }
            _ => panic!("the relayed event did not arrive"),
        }
    }

    #[test]
    fn a_failed_relay_is_silent() {
        // Nothing listens on port 1, so the connection is refused and the event is dropped.
        let target = Target {
            session: "s1".to_owned(),
            port: 1,
            token: "t".to_owned(),
        };
        forward(&target, "claude", json!({ "hook_event_name": "Stop" }));
    }

    #[test]
    fn the_event_argument_names_a_payload_that_does_not_say_it() {
        let payload = relay_payload(r#"{"tool":"x"}"#, Some("preToolUse")).expect("payload");
        assert_eq!(payload[EVENT_KEY], "preToolUse");
        assert_eq!(payload["tool"], "x");
    }

    #[test]
    fn a_payload_that_names_its_own_event_keeps_it() {
        let payload =
            relay_payload(r#"{"hook_event_name":"Stop"}"#, Some("ignored")).expect("payload");
        assert_eq!(payload[EVENT_KEY], "Stop");
    }

    #[test]
    fn input_that_is_not_json_is_dropped_unless_an_event_names_it() {
        assert!(relay_payload("not json", None).is_none());
        let named = relay_payload("not json", Some("agentSpawn")).expect("payload");
        assert_eq!(named, json!({ EVENT_KEY: "agentSpawn" }));
    }
}
