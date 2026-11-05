//! The way a tool call reaches the app. The CLI (`lynx-ide --cli`) hands each call to the running app over
//! loopback, with the session and token the agent's terminal received. Outside a Lynx terminal every call says why it
//! cannot run, and nothing else is reachable.

use std::env;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::Duration;

use serde_json::{Value, json};

use super::hook_hub::{SESSION_ENV, TOKEN_HEADER, TOOL_PATH};
use super::hub_address;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
/// A tool can wait up to 30 s for a text and 20 s for a load; this leaves room beyond both.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(120);

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
        let _ = stream.set_read_timeout(Some(ANSWER_TIMEOUT));
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
    use super::*;

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
}
