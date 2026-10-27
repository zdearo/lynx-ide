//! The ChatGPT sign-in, run in the background. The settings screen reads the status and hears
//! about each change as an event.

use std::fmt::Display;
use std::io::{ErrorKind, Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;
use tauri_specta::Event;
use uuid::Uuid;

use super::bots_events::ChatGptAuthChanged;
use super::chatgpt_credentials::{self as credentials, Credentials, Session};
use super::chatgpt_protocol::{
    AGENT_NAME, Authorization, CALLBACK_PATH, Callback, ChatGptError, JWKS_URL, Jwks,
    REGISTRATION_CLIENT, REVOKE_URL, TOKEN_URL, TokenResponse, authorize_url, code_exchange_form,
    error_code, grants_plan_use, parse_callback, pkce, revoke_form, verify_id_token,
};

/// How much of an OpenAI refusal the log keeps. A token endpoint refusal is short; a page is not.
const LOGGED_ANSWER_CHARS: usize = 4000;

/// How long the browser has to answer before the attempt gives up.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(300);
const POLL: Duration = Duration::from_millis(100);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// A browser sends its request at once after connecting; a connection that stays silent is not one.
const VISIT_TIMEOUT: Duration = Duration::from_secs(2);

/// Where the ChatGPT sign-in stands, as the settings screen shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatGptStatus {
    pub phase: ChatGptPhase,
    /// The account signed in, when there is one.
    pub email: Option<String>,
    /// Why the last attempt or sign-out did not finish cleanly, in words.
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ChatGptPhase {
    SignedOut,
    SigningIn,
    SignedIn,
    Failed,
}

pub struct ChatGptLogin {
    app: AppHandle,
    file: PathBuf,
    status: Mutex<ChatGptStatus>,
}

impl ChatGptLogin {
    /// `file` holds the credentials. A session already saved there starts out signed in.
    pub fn new(app: AppHandle, file: PathBuf) -> Self {
        let status = match credentials::read(&file).and_then(|saved| saved.session) {
            Some(session) => ChatGptStatus {
                phase: ChatGptPhase::SignedIn,
                email: session.email,
                detail: None,
            },
            None => signed_out(None),
        };
        Self {
            app,
            file,
            status: Mutex::new(status),
        }
    }

    pub fn status(&self) -> ChatGptStatus {
        self.state().clone()
    }

    /// Starts a sign-in in the background and returns at once. The outcome comes as an event.
    pub fn sign_in(self: &Arc<Self>) -> ChatGptStatus {
        {
            let mut status = self.state();
            if status.phase == ChatGptPhase::SigningIn {
                return status.clone();
            }
            status.phase = ChatGptPhase::SigningIn;
            status.detail = None;
        }
        self.announce();
        let login = Arc::clone(self);
        let started = std::thread::Builder::new()
            .name("chatgpt-sign-in".to_owned())
            .spawn(move || {
                let outcome = login.attempt();
                login.finish(outcome);
            });
        if let Err(error) = started {
            self.finish(Err(local(error)));
        }
        self.status()
    }

    /// Ends the session at OpenAI and forgets it here. The client ID stays, as OpenAI asks.
    pub fn sign_out(&self) -> ChatGptStatus {
        let next = match credentials::read(&self.file) {
            Some(saved) => self.end_session(saved),
            None => signed_out(None),
        };
        *self.state() = next.clone();
        self.announce();
        next
    }

    fn end_session(&self, mut saved: Credentials) -> ChatGptStatus {
        let Some(session) = saved.session.take() else {
            return signed_out(None);
        };
        // The local session goes either way; a failed revocation is only reported.
        let revoked = revoke(&session.refresh_token, &saved.client_id);
        let detail = revoked.err().map(|error| {
            log::error!("could not revoke the ChatGPT session: {error}");
            format!(
                "OpenAI did not confirm the sign-out ({error}). Lynx removed the session anyway."
            )
        });
        match credentials::write(&self.file, &saved) {
            Ok(()) => signed_out(detail),
            Err(error) => ChatGptStatus {
                phase: ChatGptPhase::Failed,
                email: None,
                detail: Some(format!("could not forget the session: {error}")),
            },
        }
    }

    /// One pass through the sign-in: the link, the browser, the code, the tokens, the checks, and
    /// the file. Returns the email of the account that signed in.
    fn attempt(&self) -> Result<Option<String>, ChatGptError> {
        let saved = credentials::read(&self.file);
        // The host is this install's identity, the same on every sign-in, so it is kept.
        let host_id = saved.as_ref().map_or_else(
            || format!("urn:uuid:{}", Uuid::new_v4()),
            |saved| saved.host_id.clone(),
        );
        let pair = pkce(&random_bytes());
        let state = Uuid::new_v4().to_string();
        let nonce = Uuid::new_v4().to_string();

        log::info!(
            "ChatGPT sign-in started: {}",
            if saved.is_some() {
                "with the client already registered"
            } else {
                "registering Lynx with OpenAI"
            }
        );
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(local)?;
        let port = listener.local_addr().map_err(local)?.port();
        let redirect_uri = format!("http://127.0.0.1:{port}{CALLBACK_PATH}");

        let known_client = saved
            .as_ref()
            .map_or(REGISTRATION_CLIENT, |saved| saved.client_id.as_str());
        let id_token_hint = saved
            .as_ref()
            .and_then(|saved| saved.session.as_ref())
            .map(|session| session.id_token.as_str());
        let link = authorize_url(&Authorization {
            client_id: known_client,
            agent_name_hint: saved.is_none().then_some(AGENT_NAME),
            host_id: &host_id,
            redirect_uri: &redirect_uri,
            state: &state,
            nonce: &nonce,
            challenge: &pair.challenge,
            id_token_hint,
        })
        .map_err(local)?;
        // The opener waits until the browser closes, so it runs apart: waiting here would leave the
        // listener unanswered while the browser spins.
        let opener = open::that_in_background(link.as_str());
        std::thread::spawn(move || {
            if let Ok(Err(error)) = opener.join() {
                log::error!("could not open the browser for the ChatGPT sign-in: {error}");
            }
        });

        let (code, returned_state, issued) = match wait_for_callback(&listener)? {
            Callback::Code {
                code,
                state,
                client_id,
            } => (code, state, client_id),
            Callback::Refused => return Err(ChatGptError::Refused),
        };
        if returned_state != state {
            return Err(ChatGptError::StateMismatch);
        }
        let client_id = match (&saved, issued) {
            (Some(saved), Some(issued)) if issued != saved.client_id => {
                return Err(ChatGptError::AccountChanged);
            }
            (Some(saved), _) => saved.client_id.clone(),
            (None, Some(issued)) if issued != REGISTRATION_CLIENT => issued,
            _ => return Err(ChatGptError::RegistrationIncomplete),
        };
        // Keep the issued client before the exchange, so a refused exchange retries with it instead
        // of registering a new app with OpenAI each time.
        if saved.is_none() {
            let registered = Credentials {
                client_id: client_id.clone(),
                host_id: host_id.clone(),
                session: None,
            };
            credentials::write(&self.file, &registered).map_err(|error| {
                ChatGptError::Local(format!("could not save the sign-in: {error}"))
            })?;
        }

        log::info!("ChatGPT sent the code for client {client_id}; exchanging it on {redirect_uri}");
        let tokens: TokenResponse = post_form(
            TOKEN_URL,
            code_exchange_form(&code, &pair.verifier, &client_id, &redirect_uri),
        )?;
        let keys: Jwks = get_json(JWKS_URL)?;
        let identity = verify_id_token(&tokens.id_token, &keys, &client_id, &nonce, unix_now())?;
        if !grants_plan_use(&tokens.scope) {
            return Err(ChatGptError::PlanNotEnabled);
        }
        if let Some(previous) = saved.as_ref().and_then(|saved| saved.session.as_ref())
            && previous.subject != identity.subject
        {
            return Err(ChatGptError::AccountChanged);
        }

        let email = identity.email.clone();
        let session = Session {
            subject: identity.subject,
            email: identity.email,
            id_token: tokens.id_token,
            access_token: tokens.access_token,
            refresh_token: tokens.refresh_token,
            expires_at: unix_now().saturating_add(tokens.expires_in),
            scope: tokens.scope,
        };
        let updated = Credentials {
            client_id,
            host_id,
            session: Some(session),
        };
        credentials::write(&self.file, &updated)
            .map_err(|error| ChatGptError::Local(format!("could not save the sign-in: {error}")))?;
        Ok(email)
    }

    /// Records how the attempt ended, and tells the screen.
    fn finish(&self, outcome: Result<Option<String>, ChatGptError>) {
        {
            let mut status = self.state();
            *status = match outcome {
                Ok(email) => {
                    log::info!("ChatGPT sign-in finished");
                    ChatGptStatus {
                        phase: ChatGptPhase::SignedIn,
                        email,
                        detail: None,
                    }
                }
                Err(error) => {
                    log::error!("ChatGPT sign-in did not finish: {error}");
                    ChatGptStatus {
                        phase: ChatGptPhase::Failed,
                        email: status.email.clone(),
                        detail: Some(error.to_string()),
                    }
                }
            };
        }
        self.announce();
    }

    fn announce(&self) {
        let _ = ChatGptAuthChanged {
            status: self.status(),
        }
        .emit(&self.app);
    }

    fn state(&self) -> MutexGuard<'_, ChatGptStatus> {
        self.status.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn signed_out(detail: Option<String>) -> ChatGptStatus {
    ChatGptStatus {
        phase: ChatGptPhase::SignedOut,
        email: None,
        detail,
    }
}

/// The loopback listener answers each visit. Visits that are not the callback (a favicon, say)
/// get a 404 and do not end the wait.
fn wait_for_callback(listener: &TcpListener) -> Result<Callback, ChatGptError> {
    listener.set_nonblocking(true).map_err(local)?;
    let deadline = Instant::now() + ANSWER_TIMEOUT;
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                // Some systems hand the listener's non-blocking mode to the accepted socket.
                stream.set_nonblocking(false).map_err(local)?;
                stream
                    .set_read_timeout(Some(VISIT_TIMEOUT))
                    .map_err(local)?;
                let target = request_target(&mut stream);
                let callback = target.as_deref().and_then(parse_callback);
                if callback.is_some() {
                    // Logs whether a plus sign came in the query, never the code itself.
                    let plus = target.as_deref().is_some_and(|target| target.contains('+'));
                    log::info!("the browser reached the callback (plus sign in its query: {plus})");
                }
                respond(&mut stream, callback.is_some());
                if let Some(callback) = callback {
                    return Ok(callback);
                }
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Err(ChatGptError::TimedOut);
                }
                std::thread::sleep(POLL);
            }
            Err(error) => return Err(local(error)),
        }
    }
}

/// The target of the request's first line, when it is a GET.
fn request_target(stream: &mut TcpStream) -> Option<String> {
    let mut buffer = [0u8; 4096];
    let read = stream.read(&mut buffer).ok()?;
    let head = String::from_utf8_lossy(&buffer[..read]);
    let mut parts = head.lines().next()?.split_whitespace();
    if parts.next()? != "GET" {
        return None;
    }
    parts.next().map(str::to_owned)
}

fn respond(stream: &mut TcpStream, answered: bool) {
    let (status, body) = if answered {
        (
            "200 OK",
            "Lynx received the answer. You can close this tab.",
        )
    } else {
        ("404 Not Found", "Nothing to see here.")
    };
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
}

/// 32 bytes of randomness for the PKCE verifier, from the UUID generator's operating-system source.
fn random_bytes() -> [u8; 32] {
    let mut bytes = [0u8; 32];
    bytes[..16].copy_from_slice(Uuid::new_v4().as_bytes());
    bytes[16..].copy_from_slice(Uuid::new_v4().as_bytes());
    bytes
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
        })
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(REQUEST_TIMEOUT))
        // A refusal still has a body that says why, so it is read rather than thrown away.
        .http_status_as_error(false)
        .build()
        .new_agent()
}

fn post_form<T, I, K, V>(url: &str, form: I) -> Result<T, ChatGptError>
where
    T: DeserializeOwned,
    I: IntoIterator<Item = (K, V)>,
    K: AsRef<str>,
    V: AsRef<str>,
{
    let mut response = agent().post(url).send_form(form).map_err(network)?;
    let text = answer_text(url, &mut response)?;
    serde_json::from_str(&text)
        .map_err(|error| ChatGptError::Network(format!("unreadable answer: {error}")))
}

fn get_json<T: DeserializeOwned>(url: &str) -> Result<T, ChatGptError> {
    let mut response = agent().get(url).call().map_err(network)?;
    let text = answer_text(url, &mut response)?;
    serde_json::from_str(&text)
        .map_err(|error| ChatGptError::Network(format!("unreadable answer: {error}")))
}

fn network(error: ureq::Error) -> ChatGptError {
    ChatGptError::Network(error.to_string())
}

/// The body of an answer. A status other than 2xx goes to the log in full; the error keeps only
/// OpenAI's error code, so the screen stays short.
fn answer_text(
    url: &str,
    response: &mut ureq::http::Response<ureq::Body>,
) -> Result<String, ChatGptError> {
    let text = response.body_mut().read_to_string().map_err(network)?;
    let status = response.status();
    if status.is_success() {
        return Ok(text);
    }
    let logged: String = text.chars().take(LOGGED_ANSWER_CHARS).collect();
    log::error!("OpenAI answered {status} to {url}: {logged}");
    Err(ChatGptError::Answer {
        status: status.as_u16(),
        code: error_code(&text),
    })
}

fn revoke(refresh_token: &str, client_id: &str) -> Result<(), ChatGptError> {
    let mut response = agent()
        .post(REVOKE_URL)
        .send_form(revoke_form(refresh_token, client_id))
        .map_err(network)?;
    answer_text(REVOKE_URL, &mut response).map(|_| ())
}

fn local(error: impl Display) -> ChatGptError {
    ChatGptError::Local(error.to_string())
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpStream;

    use super::*;

    #[test]
    fn the_wait_goes_on_past_other_visits_to_the_callback() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind");
        let port = listener.local_addr().expect("address").port();
        let browser = std::thread::spawn(move || {
            for target in ["/favicon.ico", "/auth/callback?code=c1&state=s1"] {
                let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).expect("connect");
                write!(stream, "GET {target} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
                    .expect("request");
                let mut answer = String::new();
                stream.read_to_string(&mut answer).expect("answer");
            }
        });

        let callback = wait_for_callback(&listener).expect("callback");

        browser.join().expect("browser");
        assert_eq!(
            callback,
            Callback::Code {
                code: "c1".to_owned(),
                state: "s1".to_owned(),
                client_id: None,
            }
        );
    }
}
