//! Loopback listener for hook reports. Each agent session gets its id, the port and a token in
//! its environment; the relay posts reports with them, and they reach that session's monitor.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;

use super::agent_monitor::AgentInput;
use super::harness::harness_named;

pub const HOOK_PATH: &str = "/agent-hook";
pub const TOKEN_HEADER: &str = "X-Lynx-Token";
pub const SESSION_ENV: &str = "LYNX_SESSION_ID";
pub const PORT_ENV: &str = "LYNX_HOOK_PORT";
pub const TOKEN_ENV: &str = "LYNX_HOOK_TOKEN";

const IO_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_HEAD_BYTES: usize = 16 * 1024;
const MAX_BODY_BYTES: usize = 1024 * 1024;

/// Clones share one listener, so every session in the app reports through the same port.
#[derive(Clone)]
pub struct HookHub {
    port: Option<u16>,
    routes: Arc<Routes>,
}

struct Routes {
    token: String,
    sessions: Mutex<HashMap<String, Sender<AgentInput>>>,
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
            }),
        }
    }

    /// The variables a session's shell gets. Empty when the listener is off.
    pub fn session_env(&self, session_id: &str) -> Vec<(String, String)> {
        let Some(port) = self.port else {
            return Vec::new();
        };
        vec![
            (SESSION_ENV.to_owned(), session_id.to_owned()),
            (PORT_ENV.to_owned(), port.to_string()),
            (TOKEN_ENV.to_owned(), self.routes.token.clone()),
        ]
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
    for stream in listener.incoming().flatten() {
        let routes = Arc::clone(routes);
        // A thread per request keeps one slow client from holding up the others.
        let _ = std::thread::Builder::new()
            .name("agent-hook-request".to_owned())
            .spawn(move || handle(stream, &routes));
    }
}

fn handle(mut stream: TcpStream, routes: &Routes) {
    let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
    let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
    let status = match read_request(&mut stream) {
        Some(request) => route(&request, routes),
        None => "400 Bad Request",
    };
    let response = format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
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

fn route(request: &Request, routes: &Routes) -> &'static str {
    if request.method != "POST" || request.path != HOOK_PATH {
        return "404 Not Found";
    }
    if request.token.as_deref() != Some(routes.token.as_str()) {
        return "401 Unauthorized";
    }
    let Ok(envelope) = serde_json::from_slice::<Envelope>(&request.body) else {
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
    fn a_disabled_hub_gives_sessions_no_hook_variables() {
        assert!(HookHub::disabled().session_env("s1").is_empty());
    }
}
