//! `lynx-ide --mcp`: the MCP server an agent starts over stdio. It answers the protocol itself and
//! hands each tool call to the running app over loopback, with the session and token the agent's
//! terminal received. Outside a Lynx terminal the tools are listed but every call says why it
//! cannot run, and nothing else is reachable.

use std::env;
use std::io::{BufRead, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::Duration;

use serde_json::{Value, json};

use super::hook_hub::{SESSION_ENV, TOKEN_HEADER, TOOL_PATH};
use super::hub_address;
use super::tool_catalog::{instructions, list};

pub const FLAG: &str = "--mcp";
const SERVER_NAME: &str = "lynx";
/// Versions this server speaks, newest first. A client asking for another gets the newest.
const PROTOCOLS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
/// A tool can wait up to 30 s for a text and 20 s for a load; this leaves room beyond both.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(120);

/// Where a tool call goes to be run.
pub trait ToolTransport {
    /// The MCP `tools/call` result for the call, or why it could not be made.
    fn call(&self, tool: &str, arguments: &Value) -> Result<Value, String>;
}

/// Runs the server when the arguments ask for it and returns the exit code. None means the app
/// should start normally.
pub fn run_if_requested(arguments: &[String]) -> Option<i32> {
    if arguments.get(1).map(String::as_str) != Some(FLAG) {
        return None;
    }
    // The agent's own name follows the flag, so Lynx can label what it does on the page.
    let transport = LoopbackTransport::from_env(arguments.get(2).cloned());
    serve(
        std::io::stdin().lock(),
        std::io::stdout().lock(),
        &transport,
    );
    Some(0)
}

/// Reads one JSON message per line and answers each that needs it, until the agent closes stdin.
pub fn serve(input: impl BufRead, mut output: impl Write, transport: &impl ToolTransport) {
    for line in input.lines() {
        let Ok(line) = line else { return };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            let reply = failure(Value::Null, -32700, "parse error");
            if write_line(&mut output, &reply).is_err() {
                return;
            }
            continue;
        };
        if let Some(reply) = answer(&message, transport)
            && write_line(&mut output, &reply).is_err()
        {
            return;
        }
    }
}

fn write_line(output: &mut impl Write, message: &Value) -> std::io::Result<()> {
    writeln!(output, "{message}")?;
    output.flush()
}

/// The reply to one message, or None for a notification, which gets none.
fn answer(message: &Value, transport: &impl ToolTransport) -> Option<Value> {
    let id = message.get("id").cloned()?;
    let method = message.get("method").and_then(Value::as_str).unwrap_or("");
    let params = message.get("params").unwrap_or(&Value::Null);
    let result = match method {
        "initialize" => json!({
            "protocolVersion": negotiated(params),
            "capabilities": { "tools": {} },
            "serverInfo": { "name": SERVER_NAME, "version": env!("CARGO_PKG_VERSION") },
            "instructions": instructions(),
        }),
        "ping" => json!({}),
        "tools/list" => json!({ "tools": list() }),
        "tools/call" => call_tool(params, transport),
        other => return Some(failure(id, -32601, &format!("method not found: {other}"))),
    };
    Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}

fn negotiated(params: &Value) -> &'static str {
    let asked = params.get("protocolVersion").and_then(Value::as_str);
    PROTOCOLS
        .iter()
        .find(|version| Some(**version) == asked)
        .copied()
        .unwrap_or(PROTOCOLS[0])
}

fn call_tool(params: &Value, transport: &impl ToolTransport) -> Value {
    let Some(name) = params.get("name").and_then(Value::as_str) else {
        return tool_error("the call has no tool name");
    };
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    transport
        .call(name, &arguments)
        .unwrap_or_else(|reason| tool_error(&reason))
}

/// A tool-level failure: the agent reads it and can adjust, unlike a protocol error.
fn tool_error(text: &str) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": true })
}

fn failure(id: Value, code: i32, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// The terminal this process runs in, as the app knows it.
struct LoopbackTransport {
    /// None outside a Lynx terminal. The hub's address is not kept: this process lives as long as
    /// its agent, and the app can restart under it, so each call looks the address up.
    session: Option<String>,
    agent: Option<String>,
}

impl LoopbackTransport {
    fn from_env(agent: Option<String>) -> Self {
        Self {
            session: env::var(SESSION_ENV).ok(),
            agent,
        }
    }
}

const NOT_IN_LYNX: &str = "The Lynx browser is only available to agents started from a terminal inside Lynx. Start this agent from a Lynx terminal and try again.";
const NOT_RUNNING: &str = "Could not reach Lynx. Check that the app is still open.";

impl ToolTransport for LoopbackTransport {
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
    use std::cell::RefCell;

    use super::*;

    struct Recorder {
        calls: RefCell<Vec<(String, Value)>>,
        reply: Result<Value, String>,
    }

    impl Recorder {
        fn replying(reply: Result<Value, String>) -> Self {
            Self {
                calls: RefCell::new(Vec::new()),
                reply,
            }
        }
    }

    impl ToolTransport for Recorder {
        fn call(&self, tool: &str, arguments: &Value) -> Result<Value, String> {
            self.calls
                .borrow_mut()
                .push((tool.to_owned(), arguments.clone()));
            self.reply.clone()
        }
    }

    /// Feeds the lines to the server and returns each line it printed, parsed.
    fn exchange(lines: &[Value], transport: &Recorder) -> Vec<Value> {
        let input = lines
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        let mut output = Vec::new();
        serve(input.as_bytes(), &mut output, transport);
        String::from_utf8(output)
            .expect("utf-8")
            .lines()
            .map(|line| serde_json::from_str(line).expect("each line is JSON"))
            .collect()
    }

    #[test]
    fn the_handshake_names_the_server_and_agrees_on_a_version() {
        let transport = Recorder::replying(Ok(json!({})));
        let replies = exchange(
            &[
                json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-03-26" } }),
                json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
                json!({ "jsonrpc": "2.0", "id": 2, "method": "ping" }),
            ],
            &transport,
        );

        assert_eq!(replies.len(), 2, "a notification gets no reply");
        assert_eq!(replies[0]["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(replies[0]["result"]["serverInfo"]["name"], "lynx");
        assert!(replies[0]["result"]["capabilities"]["tools"].is_object());
        assert_eq!(replies[1]["result"], json!({}));
    }

    #[test]
    fn an_unknown_protocol_version_gets_the_newest() {
        let transport = Recorder::replying(Ok(json!({})));
        let replies = exchange(
            &[
                json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "1999-01-01" } }),
            ],
            &transport,
        );
        assert_eq!(replies[0]["result"]["protocolVersion"], PROTOCOLS[0]);
    }

    #[test]
    fn the_tools_are_listed_without_reaching_the_app() {
        let transport = Recorder::replying(Err("unreachable".into()));
        let replies = exchange(
            &[json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/list" })],
            &transport,
        );

        let tools = replies[0]["result"]["tools"].as_array().expect("tools");
        assert!(tools.iter().any(|tool| tool["name"] == "browser_snapshot"));
        assert!(transport.calls.borrow().is_empty());
    }

    #[test]
    fn a_tool_call_is_handed_over_and_its_result_returned_as_is() {
        let result =
            json!({ "content": [{ "type": "text", "text": "Clicked e1." }], "isError": false });
        let transport = Recorder::replying(Ok(result.clone()));
        let replies = exchange(
            &[
                json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": { "name": "browser_click", "arguments": { "ref": "e1" } } }),
            ],
            &transport,
        );

        assert_eq!(replies[0]["result"], result);
        assert_eq!(
            transport.calls.borrow().as_slice(),
            [("browser_click".to_owned(), json!({ "ref": "e1" }))]
        );
    }

    #[test]
    fn a_call_that_cannot_be_made_is_an_error_the_agent_can_read() {
        let transport = Recorder::replying(Err("Could not reach Lynx.".into()));
        let replies = exchange(
            &[
                json!({ "jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": { "name": "browser_snapshot" } }),
            ],
            &transport,
        );

        assert_eq!(replies[0]["result"]["isError"], true);
        assert_eq!(
            replies[0]["result"]["content"][0]["text"],
            "Could not reach Lynx."
        );
    }

    #[test]
    fn unknown_methods_and_broken_lines_get_protocol_errors() {
        let transport = Recorder::replying(Ok(json!({})));
        let mut output = Vec::new();
        serve(
            "not json\n{\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"resources/list\"}\n".as_bytes(),
            &mut output,
            &transport,
        );
        let lines: Vec<Value> = String::from_utf8(output)
            .expect("utf-8")
            .lines()
            .map(|line| serde_json::from_str(line).expect("json"))
            .collect();

        assert_eq!(lines[0]["error"]["code"], -32700);
        assert_eq!(lines[1]["error"]["code"], -32601);
        assert_eq!(lines[1]["id"], 9);
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
    fn the_hub_answers_are_turned_into_results_or_reasons() {
        let ok = parse_response("HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}");
        assert_eq!(ok, Ok(json!({})));
        let gone = parse_response("HTTP/1.1 404 Not Found\r\n\r\n");
        assert!(gone.unwrap_err().contains("does not know this terminal"));
        let stale = parse_response("HTTP/1.1 401 Unauthorized\r\n\r\n");
        assert!(stale.unwrap_err().contains("token is stale"));
    }
}
