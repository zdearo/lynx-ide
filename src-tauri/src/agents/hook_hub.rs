//! Loopback listener for hook reports. Each agent session gets its id, the port and a token in
//! its environment; the relay posts reports with them, and they reach that session's monitor.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;

use super::agent_monitor::AgentInput;
use super::harness::harness_named;

pub const HOOK_PATH: &str = "/agent-hook";
pub const TOOL_PATH: &str = "/agent-tool";
pub const TOKEN_HEADER: &str = "X-Lynx-Token";
pub const SESSION_ENV: &str = "LYNX_SESSION_ID";
pub const PORT_ENV: &str = "LYNX_HOOK_PORT";
pub const TOKEN_ENV: &str = "LYNX_HOOK_TOKEN";
/// Where the app writes the hub's current port and token; see `hub_address`.
pub const FILE_ENV: &str = "LYNX_HUB_FILE";

const IO_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_HEAD_BYTES: usize = 16 * 1024;
const MAX_BODY_BYTES: usize = 1024 * 1024;
/// Requests served at once; a tool call can hold one for many seconds.
const MAX_CONNECTIONS: usize = 64;

/// Runs the tools an agent calls through its MCP process. The hub only authenticates the call and
/// finds the session it came from; what the tool does is up to the handler.
pub trait ToolHandler: Send + Sync + 'static {
    /// Returns the MCP `tools/call` result. Blocks while the tool works.
    /// `agent` is the harness name the agent's MCP entry carries (`claude`, `codex`...), or empty.
    fn call(&self, session_id: &str, agent: &str, tool: &str, arguments: Value) -> Value;
}

/// Clones share one listener, so every session in the app reports through the same port.
#[derive(Clone)]
pub struct HookHub {
    port: Option<u16>,
    routes: Arc<Routes>,
}

struct Routes {
    token: String,
    sessions: Mutex<HashMap<String, Sender<AgentInput>>>,
    tools: Mutex<Option<Arc<dyn ToolHandler>>>,
    /// Browser tool calls running per session, so a session in the middle of one is not frozen.
    tools_running: Mutex<HashMap<String, u32>>,
    /// Set once the address is published, so sessions can be told where to read it.
    address_file: Mutex<Option<PathBuf>>,
}

/// An answer to a request: the status line, and a JSON body when there is one.
struct Reply {
    status: &'static str,
    body: Option<String>,
}

impl Reply {
    fn status(status: &'static str) -> Self {
        Self { status, body: None }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ToolCall {
    session_id: String,
    #[serde(default)]
    agent: Option<String>,
    tool: String,
    #[serde(default)]
    arguments: Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Envelope {
    session_id: String,
    harness: String,
    payload: Value,
}

impl HookHub {
    /// Opens the loopback listener. If that fails, hooks are off and status comes from the
    /// terminal alone.
    pub fn start() -> Self {
        let routes = Arc::new(Routes {
            token: uuid::Uuid::new_v4().to_string(),
            sessions: Mutex::new(HashMap::new()),
            tools: Mutex::new(None),
            tools_running: Mutex::new(HashMap::new()),
            address_file: Mutex::new(None),
        });
        let port = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .ok()
            .and_then(|listener| {
                let port = listener.local_addr().ok()?.port();
                let served = Arc::clone(&routes);
                std::thread::Builder::new()
                    .name("agent-hook-listener".to_owned())
                    .spawn(move || accept_loop(&listener, &served))
                    .ok()
                    .map(|_| port)
            });
        Self { port, routes }
    }

    #[cfg(test)]
    pub fn disabled() -> Self {
        Self {
            port: None,
            routes: Arc::new(Routes {
                token: String::new(),
                sessions: Mutex::new(HashMap::new()),
                tools: Mutex::new(None),
                tools_running: Mutex::new(HashMap::new()),
                address_file: Mutex::new(None),
            }),
        }
    }

    /// Whether a browser tool is running for the session right now.
    pub fn tool_running(&self, session_id: &str) -> bool {
        self.routes
            .tools_running
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains_key(session_id)
    }

    /// The variables a session's shell gets. Empty when the listener is off.
    pub fn session_env(&self, session_id: &str) -> Vec<(String, String)> {
        let Some(port) = self.port else {
            return Vec::new();
        };
        let mut env = vec![
            (SESSION_ENV.to_owned(), session_id.to_owned()),
            (PORT_ENV.to_owned(), port.to_string()),
            (TOKEN_ENV.to_owned(), self.routes.token.clone()),
        ];
        if let Some(file) = self
            .routes
            .address_file
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
        {
            env.push((FILE_ENV.to_owned(), file.to_string_lossy().into_owned()));
        }
        env
    }

    /// Writes the hub's port and token to `file` and tells later sessions where it is. Without
    /// it, an agent that outlives this run keeps a port nobody listens on. A failure leaves
    /// sessions on their variables, which work for as long as this run does.
    pub fn publish_address(&self, file: &Path) -> std::io::Result<()> {
        let Some(port) = self.port else {
            return Ok(());
        };
        super::hub_address::publish(
            file,
            &super::hub_address::HubAddress {
                port,
                token: self.routes.token.clone(),
            },
        )?;
        *self
            .routes
            .address_file
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(file.to_path_buf());
        Ok(())
    }

    /// Sets who answers an agent's tool calls. Until then they get "service unavailable".
    pub fn set_tool_handler(&self, handler: Arc<dyn ToolHandler>) {
        *self
            .routes
            .tools
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(handler);
    }

    pub fn register(&self, session_id: &str, input: Sender<AgentInput>) {
        self.sessions().insert(session_id.to_owned(), input);
    }

    pub fn unregister(&self, session_id: &str) {
        self.sessions().remove(session_id);
    }

    fn sessions(&self) -> std::sync::MutexGuard<'_, HashMap<String, Sender<AgentInput>>> {
        self.routes
            .sessions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

fn accept_loop(listener: &TcpListener, routes: &Arc<Routes>) {
    let open = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming().flatten() {
        if open.fetch_add(1, Ordering::AcqRel) >= MAX_CONNECTIONS {
            open.fetch_sub(1, Ordering::AcqRel);
            continue;
        }
        let routes = Arc::clone(routes);
        let slot = Arc::clone(&open);
        // A thread per request keeps one slow client from holding up the others.
        let spawned = std::thread::Builder::new()
            .name("agent-hook-request".to_owned())
            .spawn(move || {
                handle(stream, &routes);
                slot.fetch_sub(1, Ordering::AcqRel);
            });
        if spawned.is_err() {
            open.fetch_sub(1, Ordering::AcqRel);
        }
    }
}

fn handle(mut stream: TcpStream, routes: &Routes) {
    let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
    let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
    let reply = match read_request(&mut stream) {
        Some(request) => route(&request, routes),
        None => Reply::status("400 Bad Request"),
    };
    let body = reply.body.unwrap_or_default();
    let response = format!(
        "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        reply.status,
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
}

struct Request {
    method: String,
    path: String,
    token: Option<String>,
    body: Vec<u8>,
}

fn read_request(stream: &mut TcpStream) -> Option<Request> {
    let mut buffer = Vec::new();
    let head_end = loop {
        if let Some(end) = find(&buffer, b"\r\n\r\n") {
            break end + 4;
        }
        if buffer.len() > MAX_HEAD_BYTES {
            return None;
        }
        read_more(stream, &mut buffer)?;
    };

    let head = std::str::from_utf8(&buffer[..head_end]).ok()?;
    let mut lines = head.split("\r\n");
    let mut parts = lines.next()?.split_whitespace();
    let method = parts.next()?.to_owned();
    let path = parts.next()?.to_owned();

    let mut length = 0;
    let mut token = None;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match name.trim().to_ascii_lowercase().as_str() {
            "content-length" => length = value.parse::<usize>().ok()?,
            "x-lynx-token" => token = Some(value.to_owned()),
            _ => {}
        }
    }
    if length > MAX_BODY_BYTES {
        return None;
    }
    while buffer.len() < head_end + length {
        read_more(stream, &mut buffer)?;
    }
    let body = buffer[head_end..head_end + length].to_vec();
    Some(Request {
        method,
        path,
        token,
        body,
    })
}

fn read_more(stream: &mut TcpStream, buffer: &mut Vec<u8>) -> Option<()> {
    let mut chunk = [0u8; 4096];
    let read = stream.read(&mut chunk).ok()?;
    if read == 0 {
        return None;
    }
    buffer.extend_from_slice(&chunk[..read]);
    Some(())
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn route(request: &Request, routes: &Routes) -> Reply {
    if request.method != "POST" || !(request.path == HOOK_PATH || request.path == TOOL_PATH) {
        return Reply::status("404 Not Found");
    }
    if !token_matches(request.token.as_deref(), &routes.token) {
        return Reply::status("401 Unauthorized");
    }
    if request.path == TOOL_PATH {
        return route_tool(&request.body, routes);
    }
    Reply::status(route_hook(&request.body, routes))
}

/// Compares without stopping at the first differing byte, so timing does not leak the token.
fn token_matches(given: Option<&str>, expected: &str) -> bool {
    let Some(given) = given else {
        return false;
    };
    let (given, expected) = (given.as_bytes(), expected.as_bytes());
    let mut diff = given.len() ^ expected.len();
    for (index, byte) in expected.iter().enumerate() {
        diff |= usize::from(byte ^ given.get(index).copied().unwrap_or(0));
    }
    diff == 0
}

fn route_hook(body: &[u8], routes: &Routes) -> &'static str {
    let Ok(envelope) = serde_json::from_slice::<Envelope>(body) else {
        return "400 Bad Request";
    };
    let Some(harness) = harness_named(&envelope.harness) else {
        return "400 Bad Request";
    };
    let sessions = routes
        .sessions
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let Some(input) = sessions.get(&envelope.session_id) else {
        return "404 Not Found";
    };
    let report = AgentInput::Hook {
        harness,
        payload: envelope.payload,
    };
    if input.send(report).is_ok() {
        "204 No Content"
    } else {
        "404 Not Found"
    }
}

/// A tool call from a session's MCP process. Only a session that is open in Lynx may call.
/// Adds or removes one running tool call for the session; a count, since calls can overlap.
fn count_tool(routes: &Routes, session_id: &str, change: i32) {
    let mut running = routes
        .tools_running
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let count = running.entry(session_id.to_owned()).or_insert(0);
    *count = count.saturating_add_signed(change);
    if *count == 0 {
        running.remove(session_id);
    }
}

struct ToolRun<'a> {
    routes: &'a Routes,
    session_id: &'a str,
}

impl<'a> ToolRun<'a> {
    fn start(routes: &'a Routes, session_id: &'a str) -> Self {
        count_tool(routes, session_id, 1);
        Self { routes, session_id }
    }
}

impl Drop for ToolRun<'_> {
    fn drop(&mut self) {
        count_tool(self.routes, self.session_id, -1);
    }
}

fn route_tool(body: &[u8], routes: &Routes) -> Reply {
    let Ok(call) = serde_json::from_slice::<ToolCall>(body) else {
        return Reply::status("400 Bad Request");
    };
    let known = routes
        .sessions
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .contains_key(&call.session_id);
    if !known {
        return Reply::status("404 Not Found");
    }
    let handler = routes
        .tools
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    let Some(handler) = handler else {
        return Reply::status("503 Service Unavailable");
    };
    // The lock is released: a tool can take many seconds, and other sessions keep reporting.
    // Dropped even if the tool panics, so the session is never left marked busy.
    let _running = ToolRun::start(routes, &call.session_id);
    let result = handler.call(
        &call.session_id,
        call.agent.as_deref().unwrap_or_default(),
        &call.tool,
        call.arguments,
    );
    Reply {
        status: "200 OK",
        body: Some(result.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;
    use crate::agents::harness::HarnessId;

    /// Posts raw HTTP to the hub and returns the status line it answered with.
    fn post(port: u16, token: &str, body: &str) -> String {
        let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).expect("connect");
        let request = format!(
            "POST {HOOK_PATH} HTTP/1.1\r\nHost: 127.0.0.1\r\n{TOKEN_HEADER}: {token}\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(request.as_bytes()).expect("write");
        let mut answer = String::new();
        stream.read_to_string(&mut answer).expect("read");
        answer.lines().next().unwrap_or_default().to_owned()
    }

    fn env_value(env: &[(String, String)], key: &str) -> String {
        env.iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.clone())
            .expect("variable is set")
    }

    #[test]
    fn a_report_with_the_token_reaches_its_session() {
        let hub = HookHub::start();
        let (input, inbox) = mpsc::channel();
        hub.register("s1", input);
        let env = hub.session_env("s1");
        let port: u16 = env_value(&env, PORT_ENV).parse().expect("port");

        let status = post(
            port,
            &env_value(&env, TOKEN_ENV),
            r#"{"sessionId":"s1","harness":"claude","payload":{"hook_event_name":"Stop"}}"#,
        );

        assert_eq!(status, "HTTP/1.1 204 No Content");
        match inbox.recv_timeout(Duration::from_secs(2)) {
            Ok(AgentInput::Hook { harness, payload }) => {
                assert_eq!(harness, HarnessId::Claude);
                assert_eq!(payload["hook_event_name"], "Stop");
            }
            _ => panic!("the report did not reach the session"),
        }
    }

    #[test]
    fn a_wrong_token_is_refused_and_nothing_is_delivered() {
        let hub = HookHub::start();
        let (input, inbox) = mpsc::channel();
        hub.register("s1", input);
        let port: u16 = env_value(&hub.session_env("s1"), PORT_ENV)
            .parse()
            .expect("port");

        let status = post(
            port,
            "not-the-token",
            r#"{"sessionId":"s1","harness":"claude","payload":{}}"#,
        );

        assert_eq!(status, "HTTP/1.1 401 Unauthorized");
        assert!(inbox.recv_timeout(Duration::from_millis(100)).is_err());
    }

    #[test]
    fn a_report_for_an_unknown_session_is_not_found() {
        let hub = HookHub::start();
        let env = hub.session_env("nobody");
        let port: u16 = env_value(&env, PORT_ENV).parse().expect("port");

        let status = post(
            port,
            &env_value(&env, TOKEN_ENV),
            r#"{"sessionId":"nobody","harness":"claude","payload":{}}"#,
        );

        assert_eq!(status, "HTTP/1.1 404 Not Found");
    }

    #[test]
    fn a_session_that_is_unregistered_stops_receiving_reports() {
        let hub = HookHub::start();
        let (input, _inbox) = mpsc::channel();
        hub.register("s1", input);
        hub.unregister("s1");
        let env = hub.session_env("s1");
        let port: u16 = env_value(&env, PORT_ENV).parse().expect("port");

        let status = post(
            port,
            &env_value(&env, TOKEN_ENV),
            r#"{"sessionId":"s1","harness":"claude","payload":{}}"#,
        );

        assert_eq!(status, "HTTP/1.1 404 Not Found");
    }

    #[test]
    fn a_report_from_an_unknown_harness_is_refused() {
        let hub = HookHub::start();
        let (input, inbox) = mpsc::channel();
        hub.register("s1", input);
        let env = hub.session_env("s1");
        let port: u16 = env_value(&env, PORT_ENV).parse().expect("port");

        let status = post(
            port,
            &env_value(&env, TOKEN_ENV),
            r#"{"sessionId":"s1","harness":"mystery","payload":{}}"#,
        );

        assert_eq!(status, "HTTP/1.1 400 Bad Request");
        assert!(inbox.recv_timeout(Duration::from_millis(100)).is_err());
    }

    #[test]
    fn a_published_address_matches_the_hub_and_sessions_are_told_where_it_is() {
        let hub = HookHub::start();
        assert!(
            !hub.session_env("s1")
                .iter()
                .any(|(name, _)| name == FILE_ENV),
            "nothing is published yet"
        );
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("hub.json");

        hub.publish_address(&file).expect("publish");

        let env = hub.session_env("s1");
        let address = super::super::hub_address::read(&file).expect("address");
        assert_eq!(env_value(&env, FILE_ENV), file.to_string_lossy());
        assert_eq!(env_value(&env, PORT_ENV), address.port.to_string());
        assert_eq!(env_value(&env, TOKEN_ENV), address.token);
    }

    #[test]
    fn a_disabled_hub_publishes_nothing() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("hub.json");

        HookHub::disabled().publish_address(&file).expect("publish");

        assert!(!file.exists());
    }

    #[test]
    fn a_disabled_hub_gives_sessions_no_hook_variables() {
        assert!(HookHub::disabled().session_env("s1").is_empty());
    }

    struct Echo;

    impl ToolHandler for Echo {
        fn call(&self, session_id: &str, agent: &str, tool: &str, arguments: Value) -> Value {
            serde_json::json!({ "session": session_id, "agent": agent, "tool": tool, "arguments": arguments })
        }
    }

    /// Posts to the tool path and returns the status line and the body.
    fn post_tool(port: u16, token: &str, body: &str) -> (String, String) {
        let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).expect("connect");
        let request = format!(
            "POST {TOOL_PATH} HTTP/1.1\r\nHost: 127.0.0.1\r\n{TOKEN_HEADER}: {token}\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(request.as_bytes()).expect("write");
        let mut answer = String::new();
        stream.read_to_string(&mut answer).expect("read");
        let (head, body) = answer.split_once("\r\n\r\n").unwrap_or((&answer, ""));
        (
            head.lines().next().unwrap_or_default().to_owned(),
            body.to_owned(),
        )
    }

    fn hub_with_session(handler: bool) -> (HookHub, u16, String) {
        let hub = HookHub::start();
        let (input, _inbox) = mpsc::channel();
        hub.register("s1", input);
        if handler {
            hub.set_tool_handler(Arc::new(Echo));
        }
        let env = hub.session_env("s1");
        let port = env_value(&env, PORT_ENV).parse().expect("port");
        (hub, port, env_value(&env, TOKEN_ENV))
    }

    #[test]
    fn a_tool_call_from_a_known_session_reaches_the_handler_and_gets_its_answer() {
        let (_hub, port, token) = hub_with_session(true);

        let (status, body) = post_tool(
            port,
            &token,
            r#"{"sessionId":"s1","agent":"claude","tool":"browser_click","arguments":{"ref":"e1"}}"#,
        );

        assert_eq!(status, "HTTP/1.1 200 OK");
        let answer: Value = serde_json::from_str(&body).expect("json body");
        assert_eq!(answer["session"], "s1");
        assert_eq!(answer["agent"], "claude");
        assert_eq!(answer["tool"], "browser_click");
        assert_eq!(answer["arguments"]["ref"], "e1");
    }

    #[test]
    fn a_tool_call_needs_the_token_and_a_session_that_is_open() {
        let (_hub, port, token) = hub_with_session(true);

        let (wrong, _) = post_tool(port, "nope", r#"{"sessionId":"s1","tool":"t"}"#);
        let (stranger, _) = post_tool(port, &token, r#"{"sessionId":"other","tool":"t"}"#);
        let (malformed, _) = post_tool(port, &token, "not json");

        assert_eq!(wrong, "HTTP/1.1 401 Unauthorized");
        assert_eq!(stranger, "HTTP/1.1 404 Not Found");
        assert_eq!(malformed, "HTTP/1.1 400 Bad Request");
    }

    #[test]
    fn a_tool_call_before_a_handler_is_set_is_unavailable() {
        let (_hub, port, token) = hub_with_session(false);

        let (status, _) = post_tool(port, &token, r#"{"sessionId":"s1","tool":"t"}"#);

        assert_eq!(status, "HTTP/1.1 503 Service Unavailable");
    }
}

#[cfg(test)]
mod tool_count_tests {
    use super::*;

    /// A session counts as busy while any of its tool calls runs, however they overlap.
    #[test]
    fn a_tool_that_panics_does_not_leave_the_session_busy() {
        let hub = HookHub::start();
        let routes = Arc::clone(&hub.routes);
        let result = std::thread::spawn(move || {
            let _running = ToolRun::start(&routes, "s1");
            panic!("tool failed");
        })
        .join();
        assert!(result.is_err());
        assert!(!hub.tool_running("s1"));
    }

    #[test]
    fn only_the_exact_token_matches() {
        assert!(token_matches(Some("abc"), "abc"));
        assert!(!token_matches(Some("abd"), "abc"));
        assert!(!token_matches(Some("ab"), "abc"));
        assert!(!token_matches(Some("abcd"), "abc"));
        assert!(!token_matches(None, "abc"));
    }

    #[test]
    fn connections_past_the_limit_are_dropped_and_slots_come_back() {
        let hub = HookHub::start();
        let port = hub.port.expect("port");
        // Silent clients hold their slots until the read timeout.
        let held: Vec<_> = (0..MAX_CONNECTIONS)
            .map(|_| TcpStream::connect((Ipv4Addr::LOCALHOST, port)).expect("connect"))
            .collect();
        std::thread::sleep(Duration::from_millis(200));
        let mut extra = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).expect("connect");
        extra
            .set_read_timeout(Some(Duration::from_secs(1)))
            .expect("timeout");
        let mut answer = String::new();
        // Closed without a reply.
        assert!(matches!(extra.read_to_string(&mut answer), Ok(0)));
        drop(held);
    }

    #[test]
    fn a_session_is_busy_until_its_last_tool_call_ends() {
        let hub = HookHub::start();
        assert!(!hub.tool_running("s1"));

        count_tool(&hub.routes, "s1", 1);
        count_tool(&hub.routes, "s1", 1);
        count_tool(&hub.routes, "s1", -1);
        assert!(hub.tool_running("s1"));

        count_tool(&hub.routes, "s1", -1);
        assert!(!hub.tool_running("s1"));
        assert!(!hub.tool_running("other"));
    }
}
