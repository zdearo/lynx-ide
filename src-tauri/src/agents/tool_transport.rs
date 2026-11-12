//! The way a tool call reaches the app. The CLI (`lynx-ide --cli`) hands each call to the running app over
//! loopback, with the session and token the agent's terminal received. Outside a Lynx terminal every call says why it
//! cannot run, and nothing else is reachable.

use std::env;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::Duration;

use serde_json::{Value, json};

use super::hook_hub::{SESSION_ENV, TOKEN_HEADER, TOOL_PATH};
use super::hub_address::{self, HubAddress};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
/// A tool can wait up to 30 s for a text and 20 s for a load; this leaves room beyond both.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(120);
/// Room past the wait a call asks for in `waitSeconds`, so the answer comes after the tool's own wait ends.
const ANSWER_MARGIN: Duration = Duration::from_secs(30);

/// Where a tool call goes to be run.
pub trait ToolTransport {
    /// The MCP `tools/call` result for the call, or why it could not be made.
    fn call(&self, tool: &str, arguments: &Value) -> Result<Value, String>;
    /// The session this process serves, when it has one. A bot's session starts with `bot:`.
    fn caller(&self) -> Option<&str> {
        None
    }
}

/// The terminal this process runs in, as the app knows it.
pub struct LoopbackTransport {
    /// None outside a Lynx terminal. The hub's address is not kept: this process lives as long as
    /// its agent, and the app can restart under it, so each call looks the address up.
    session: Option<String>,
    agent: Option<String>,
}

impl LoopbackTransport {
    pub(crate) fn from_env(agent: Option<String>) -> Self {
        Self {
            session: env::var(SESSION_ENV).ok(),
            agent,
        }
    }
}

const NOT_IN_LYNX: &str = "The Lynx browser is only available to agents started from a terminal inside Lynx. Start this agent from a Lynx terminal and try again.";
const NOT_RUNNING: &str = "Could not reach Lynx. Check that the app is still open.";

impl ToolTransport for LoopbackTransport {
    fn caller(&self) -> Option<&str> {
        self.session.as_deref()
    }

    fn call(&self, tool: &str, arguments: &Value) -> Result<Value, String> {
        let session = self.session.as_ref().ok_or(NOT_IN_LYNX)?;
        let hub = hub_address::current().ok_or(NOT_RUNNING)?;
        self.call_hub(&hub, session, tool, arguments, ANSWER_TIMEOUT)
    }
}

impl LoopbackTransport {
    /// Sends one call to `hub` and waits for its answer, up to the deadline `answer_deadline` gives it from `base`.
    fn call_hub(
        &self,
        hub: &HubAddress,
        session: &str,
        tool: &str,
        arguments: &Value,
        base: Duration,
    ) -> Result<Value, String> {
        let body = json!({
            "sessionId": session,
            "agent": self.agent,
            "tool": tool,
            "arguments": arguments,
        })
        .to_string();
        let address = SocketAddr::from((Ipv4Addr::LOCALHOST, hub.port));
        let mut stream = TcpStream::connect_timeout(&address, CONNECT_TIMEOUT)
            .map_err(|_| NOT_RUNNING.to_owned())?;
        let _ = stream.set_read_timeout(Some(answer_deadline(base, arguments)));
        let _ = stream.set_write_timeout(Some(CONNECT_TIMEOUT));
        let request = format!(
            "POST {TOOL_PATH} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\n{TOKEN_HEADER}: {token}\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n{body}",
            token = hub.token,
            length = body.len(),
        );
        stream
            .write_all(request.as_bytes())
            .map_err(|_| NOT_RUNNING.to_owned())?;
        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .map_err(|_| "Lynx did not answer in time.".to_owned())?;
        parse_response(&response)
    }
}

/// How long the answer may take: the wait the call asks for in `waitSeconds` plus the margin, and never less than `base`.
fn answer_deadline(base: Duration, arguments: &Value) -> Duration {
    let wait = arguments
        .get("waitSeconds")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    base.max(Duration::from_secs(wait).saturating_add(ANSWER_MARGIN))
}

/// The result inside an HTTP answer from the hub, or what went wrong in words for the agent.
fn parse_response(response: &str) -> Result<Value, String> {
    let (head, body) = response.split_once("\r\n\r\n").unwrap_or((response, ""));
    let status = head.lines().next().unwrap_or_default();
    if status.contains(" 200 ") {
        return serde_json::from_str(body)
            .map_err(|_| "Lynx sent an unreadable answer.".to_owned());
    }
    Err(if status.contains(" 404 ") {
        "Lynx does not know this terminal any more. Start the agent from a Lynx terminal again."
            .to_owned()
    } else if status.contains(" 401 ") {
        "Lynx refused this call: the terminal's token is stale. Restart the agent from Lynx."
            .to_owned()
    } else if status.contains(" 503 ") {
        "The Lynx browser is not ready yet. Try again in a moment.".to_owned()
    } else {
        format!("Lynx could not run the tool ({status}).")
    })
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, mpsc};

    use super::*;
    use crate::agents::hook_hub::{HookHub, PORT_ENV, TOKEN_ENV, ToolHandler};

    #[test]
    fn the_hub_answers_are_turned_into_results_or_reasons() {
        let ok = parse_response("HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}");
        assert_eq!(ok, Ok(json!({})));
        let gone = parse_response("HTTP/1.1 404 Not Found\r\n\r\n");
        assert!(gone.unwrap_err().contains("does not know this terminal"));
        let stale = parse_response("HTTP/1.1 401 Unauthorized\r\n\r\n");
        assert!(stale.unwrap_err().contains("token is stale"));
    }

    #[test]
    fn outside_a_lynx_terminal_a_call_explains_itself() {
        let transport = LoopbackTransport {
            session: None,
            agent: None,
        };
        let reason = transport.call("browser_snapshot", &json!({})).unwrap_err();
        assert!(reason.contains("terminal inside Lynx"));
    }

    #[test]
    fn a_slow_call_that_asks_for_a_wait_is_answered_after_the_base_deadline() {
        struct Slow;
        impl ToolHandler for Slow {
            fn call(
                &self,
                _session_id: &str,
                _agent: &str,
                _tool: &str,
                _arguments: Value,
            ) -> Value {
                // Longer than the hub's own 2 s socket timeout: the hub must not cut a call that is still working.
                std::thread::sleep(Duration::from_millis(2_500));
                json!({ "content": [{ "type": "text", "text": "done" }], "isError": false })
            }
        }
        let hub = HookHub::start();
        let (input, _inbox) = mpsc::channel();
        hub.register("s1", input);
        hub.set_tool_handler(Arc::new(Slow));
        let env = hub.session_env("s1");
        let port = env
            .iter()
            .find(|(name, _)| name == PORT_ENV)
            .and_then(|(_, value)| value.parse().ok())
            .expect("port");
        let token = env
            .iter()
            .find(|(name, _)| name == TOKEN_ENV)
            .expect("token")
            .1
            .clone();
        let transport = LoopbackTransport {
            session: Some("s1".to_owned()),
            agent: None,
        };

        // 300 ms stands in for the base deadline a call gets when it asks for no wait (120 s), so the test stays fast.
        let answer = transport.call_hub(
            &HubAddress { port, token },
            "s1",
            "lynx_check",
            &json!({ "waitSeconds": 3 }),
            Duration::from_millis(300),
        );

        assert_eq!(
            answer,
            Ok(json!({ "content": [{ "type": "text", "text": "done" }], "isError": false }))
        );
    }

    #[test]
    fn the_answer_deadline_follows_the_wait_a_call_asks_for() {
        let base = ANSWER_TIMEOUT;
        // A call with no wait keeps the base; lynx_check's longest wait (90 s) still fits inside it.
        assert_eq!(answer_deadline(base, &json!({})), base);
        assert_eq!(answer_deadline(base, &json!({ "waitSeconds": 90 })), base);
        // A 10 min wait gets its wait plus the margin.
        assert_eq!(
            answer_deadline(base, &json!({ "waitSeconds": 600 })),
            Duration::from_secs(630)
        );
        // Not a whole number of seconds: no wait is asked, so the base applies.
        assert_eq!(
            answer_deadline(base, &json!({ "waitSeconds": "soon" })),
            base
        );
        // An absurd wait saturates instead of overflowing.
        assert!(answer_deadline(base, &json!({ "waitSeconds": u64::MAX })) > base);
    }
}
