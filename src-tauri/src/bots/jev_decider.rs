//! Who answers a channel message that names nobody, decided by Jev, TypeSafe's decision model. Jev
//! never writes the answer: it only says how likely each member is to be the right one, and the member
//! then answers on its own engine. It needs the user's own key, kept in `jev_key`. Anything short of a
//! sure answer within the deadline is a fallback, so the model can slow a channel down by no more than
//! its deadline.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use specta::Type;

use super::jev_key;

const ENDPOINT: &str = "https://api.typesafe.ai/v1/systemone";
const MODEL: &str = "jev-latest";
const QUESTION: &str = "roomRouting";
/// How long a channel waits for Jev before it goes on without it.
const TIMEOUT: Duration = Duration::from_millis(1500);
/// The test button waits longer, so it can tell a slow key from a bad one.
const TEST_TIMEOUT: Duration = Duration::from_secs(10);
/// Jev answered at or above this were right 92 to 99% of the time in MausBot's calibration.
const MIN_PROBABILITY: f64 = 0.6;
/// Jev rounds to two places, so the probabilities can sum to 0.98 or 1.02.
const SUM_TOLERANCE: f64 = 0.1;
const EVERYONE: &str = "__everyone__";
const INSTRUCTIONS: &str = "Which bot in this room should answer `new_message`? Choose __everyone__ only when the message needs several members to answer.";
const EVERYONE_MEANING: &str = "Several members: the message explicitly needs answers or work from more than one member of the room, for example it asks everyone or asks each member for their part.";
const NAME_MAX: usize = 80;
const TITLE_MAX: usize = 120;
const DESCRIPTION_MAX: usize = 400;
const LINE_MAX: usize = 500;
const MESSAGE_MAX: usize = 8000;
const RECENT_BUDGET: usize = 6000;

/// A member the question offers.
pub struct Member {
    pub id: String,
    pub name: String,
    pub title: Option<String>,
    pub description: String,
}

/// What Jev is asked: the room, who is in it, the latest lines, and the message to place.
pub struct Question {
    pub room: String,
    pub humans: Vec<String>,
    pub members: Vec<Member>,
    /// Oldest first, as (who, text).
    pub recent: Vec<(String, String)>,
    pub message: (String, String),
}

/// Who answers, as far as the decider can say.
#[derive(Debug, Clone, PartialEq)]
pub enum Route {
    /// The decider is not in use (switched off, or no key), so the channel routes as it always did.
    Off,
    Member {
        id: String,
        probability: f64,
    },
    Everyone {
        probability: f64,
    },
    /// Jev was asked and did not give a sure answer: the channel falls back to its first member.
    Fallback(&'static str),
}

pub trait RoomDecider: Send + Sync {
    fn choose(&self, question: &Question) -> Route;
}

/// Jev over its HTTP API, with the key from `key_file` and the switch read when a question is asked.
pub struct JevDecider {
    key_file: PathBuf,
    enabled: Arc<dyn Fn() -> bool + Send + Sync>,
    endpoint: String,
}

impl JevDecider {
    pub fn new(key_file: PathBuf, enabled: Arc<dyn Fn() -> bool + Send + Sync>) -> Self {
        Self {
            key_file,
            enabled,
            endpoint: ENDPOINT.to_owned(),
        }
    }
}

impl RoomDecider for JevDecider {
    fn choose(&self, question: &Question) -> Route {
        if !(self.enabled)() {
            return Route::Off;
        }
        let Some(key) = jev_key::read(&self.key_file) else {
            return Route::Off;
        };
        if question.members.len() < 2 {
            return Route::Fallback("nothing to choose");
        }
        let started = Instant::now();
        let route = decide(&self.endpoint, &key, question, TIMEOUT);
        // Never the message or the key: only how it went.
        log::info!(
            "jev routing took {} ms: {}",
            started.elapsed().as_millis(),
            match &route {
                Route::Fallback(reason) => reason,
                _ => "picked",
            }
        );
        route
    }
}

/// What the Settings test reports: whether the key works, and how long Jev took.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct JevTest {
    pub ok: bool,
    pub latency_ms: u32,
    pub reason: Option<String>,
}

/// Asks Jev a fixed question with the saved key. A sure answer is not needed: reaching Jev and
/// reading its answer is.
pub fn test(key_file: &std::path::Path) -> JevTest {
    let Some(key) = jev_key::read(key_file) else {
        return JevTest {
            ok: false,
            latency_ms: 0,
            reason: Some("no key saved".to_owned()),
        };
    };
    let question = Question {
        room: "test".to_owned(),
        humans: vec!["Person".to_owned()],
        members: ["Ada", "Bento"]
            .iter()
            .map(|name| Member {
                id: name.to_lowercase(),
                name: (*name).to_owned(),
                title: None,
                description: String::new(),
            })
            .collect(),
        recent: Vec::new(),
        message: ("Person".to_owned(), "Ada, are you there?".to_owned()),
    };
    let (body, offered) = request_body(&question);
    let started = Instant::now();
    let answer = call(ENDPOINT, &key, &body, TEST_TIMEOUT)
        .and_then(|value| parse_choice(&value, &offered).ok_or("malformed"));
    JevTest {
        ok: answer.is_ok(),
        latency_ms: u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX),
        reason: answer.err().map(str::to_owned),
    }
}

fn decide(endpoint: &str, key: &str, question: &Question, timeout: Duration) -> Route {
    let (body, offered) = request_body(question);
    match call(endpoint, key, &body, timeout) {
        Ok(value) => route_of(parse_choice(&value, &offered)),
        Err(reason) => Route::Fallback(reason),
    }
}

fn route_of(answer: Option<(String, f64)>) -> Route {
    let Some((choice, probability)) = answer else {
        return Route::Fallback("malformed");
    };
    if probability < MIN_PROBABILITY {
        return Route::Fallback("not sure");
    }
    if choice == EVERYONE {
        Route::Everyone { probability }
    } else {
        Route::Member {
            id: choice,
            probability,
        }
    }
}

/// The request, and the options it offers. The key is not in it.
fn request_body(question: &Question) -> (Value, Vec<String>) {
    let mut criteria = Map::new();
    for member in &question.members {
        criteria.insert(member.id.clone(), json!(option_text(member)));
    }
    criteria.insert(EVERYONE.to_owned(), json!(EVERYONE_MEANING));
    let offered = criteria.keys().cloned().collect();
    let mut state = Map::new();
    state.insert("room".to_owned(), json!(clip(&question.room, NAME_MAX)));
    state.insert(
        "humans_in_room".to_owned(),
        json!(
            question
                .humans
                .iter()
                .map(|human| clip(human, NAME_MAX))
                .collect::<Vec<_>>()
        ),
    );
    state.insert(
        "bots_in_room".to_owned(),
        json!(
            question
                .members
                .iter()
                .map(|member| clip(&member.name, NAME_MAX))
                .collect::<Vec<_>>()
        ),
    );
    let recent = recent_within_budget(&question.recent);
    if !recent.is_empty() {
        state.insert("recent_messages".to_owned(), json!(recent));
    }
    state.insert(
        "new_message".to_owned(),
        json!({
            "from": clip(&question.message.0, NAME_MAX),
            "text": question.message.1.trim().chars().take(MESSAGE_MAX).collect::<String>(),
        }),
    );
    let body = json!({
        "state": state,
        "model": MODEL,
        "questions": { QUESTION: { "type": "choice", "instructions": INSTRUCTIONS, "criteria": criteria } },
    });
    (body, offered)
}

fn option_text(member: &Member) -> String {
    let name = clip(&member.name, NAME_MAX);
    let title = member
        .title
        .as_deref()
        .map(|title| format!(", {}", clip(title, TITLE_MAX)))
        .unwrap_or_default();
    let description = clip(&member.description, DESCRIPTION_MAX);
    let description = if description.is_empty() {
        String::new()
    } else {
        format!(" {description}")
    };
    format!("{name}{title} bot.{description}")
}

/// The newest lines that fit the budget, oldest first.
fn recent_within_budget(recent: &[(String, String)]) -> Vec<Value> {
    let mut kept = Vec::new();
    let mut used = 0;
    for (from, text) in recent.iter().rev() {
        let text = clip(text, LINE_MAX);
        if text.is_empty() {
            continue;
        }
        used += text.chars().count() + from.chars().count();
        if used > RECENT_BUDGET {
            break;
        }
        kept.push(json!({ "from": clip(from, NAME_MAX), "text": text }));
    }
    kept.reverse();
    kept
}

fn clip(value: &str, max: usize) -> String {
    let flat = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > max {
        let cut: String = flat.chars().take(max - 1).collect();
        format!("{cut}…")
    } else {
        flat
    }
}

/// The answer, only if it can be trusted: an option that was offered, probabilities that add up, and
/// a choice that is the most likely one.
fn parse_choice(body: &Value, offered: &[String]) -> Option<(String, f64)> {
    let answer = body.get("answers")?.get(QUESTION)?.as_object()?;
    if answer.get("type").is_some_and(|kind| kind != "choice") {
        return None;
    }
    let choice = answer.get("choice")?.as_str()?;
    if !offered.iter().any(|option| option == choice) {
        return None;
    }
    let probabilities = answer.get("probabilities")?.as_object()?;
    let mut sum = 0.0;
    for (option, value) in probabilities {
        let probability = value.as_f64()?;
        if !offered.contains(option) || !(0.0..=1.0).contains(&probability) {
            return None;
        }
        sum += probability;
    }
    if (sum - 1.0).abs() > SUM_TOLERANCE {
        return None;
    }
    let top = probabilities.get(choice)?.as_f64()?;
    let runner_up = probabilities
        .iter()
        .filter(|(option, _)| option.as_str() != choice)
        .filter_map(|(_, value)| value.as_f64())
        .fold(0.0, f64::max);
    // A choice that is not the most likely option is not what it claims to be.
    (runner_up <= top + 1e-9).then(|| (choice.to_owned(), top))
}

/// One request. The reason for a failure is a fixed sentence: a body from the server can echo the
/// request, so it is never read into one.
fn call(endpoint: &str, key: &str, body: &Value, timeout: Duration) -> Result<Value, &'static str> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        // The key is never replayed to wherever a redirect points.
        .max_redirects(0)
        .build()
        .new_agent();
    let mut response = agent
        .post(endpoint)
        .header("authorization", format!("Bearer {key}"))
        .header("content-type", "application/json")
        .header("accept", "application/json")
        .send(body.to_string())
        .map_err(|error| match error {
            ureq::Error::Timeout(_) => "timed out",
            _ => "unreachable",
        })?;
    match response.status().as_u16() {
        200..=299 => {}
        401 | 403 => return Err("key rejected"),
        413 => return Err("too large"),
        429 => return Err("rate limited"),
        503 | 529 => return Err("overloaded"),
        _ => return Err("http error"),
    }
    let text = response
        .body_mut()
        .read_to_string()
        .map_err(|_| "unreachable")?;
    serde_json::from_str(&text).map_err(|_| "malformed")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;

    fn question() -> Question {
        let member = |id: &str, name: &str, description: &str| Member {
            id: id.to_owned(),
            name: name.to_owned(),
            title: Some("engineering".to_owned()),
            description: description.to_owned(),
        };
        Question {
            room: "dev".to_owned(),
            humans: vec!["Pessoa".to_owned()],
            members: vec![
                member("ada", "Ada", "Cuida do backend."),
                member("bento", "Bento", "Cuida da interface."),
            ],
            recent: vec![("Pessoa".to_owned(), "oi".to_owned())],
            message: ("Pessoa".to_owned(), "quem revisa o botão?".to_owned()),
        }
    }

    fn answer(choice: &str, ada: f64, bento: f64, everyone: f64) -> Value {
        json!({ "answers": { QUESTION: {
            "type": "choice",
            "choice": choice,
            "probabilities": { "ada": ada, "bento": bento, EVERYONE: everyone },
        } } })
    }

    /// A one-shot server: it reads the request, waits `delay`, and answers with `status` and `body`.
    fn serve(
        status: &'static str,
        body: String,
        delay: Duration,
    ) -> (String, mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
        let endpoint = format!(
            "http://{}/v1/systemone",
            listener.local_addr().expect("address")
        );
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut request = Vec::new();
            let mut chunk = [0u8; 2048];
            while let Ok(read) = stream.read(&mut chunk) {
                request.extend_from_slice(&chunk[..read]);
                let text = String::from_utf8_lossy(&request).to_lowercase();
                let complete = text.split_once("\r\n\r\n").is_some_and(|(head, rest)| {
                    let length = head
                        .lines()
                        .find_map(|line| line.strip_prefix("content-length: "))
                        .and_then(|value| value.trim().parse::<usize>().ok())
                        .unwrap_or(0);
                    rest.len() >= length
                });
                if read == 0 || complete {
                    break;
                }
            }
            let _ = sender.send(String::from_utf8_lossy(&request).into_owned());
            std::thread::sleep(delay);
            let response = format!(
                "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        });
        (endpoint, receiver)
    }

    #[test]
    fn the_request_offers_each_member_and_everyone_and_leaves_the_key_out() {
        let (body, offered) = request_body(&question());

        assert_eq!(offered, ["ada", "bento", EVERYONE]);
        assert_eq!(body["model"], MODEL);
        let asked = &body["questions"][QUESTION];
        assert_eq!(asked["type"], "choice");
        assert_eq!(
            asked["criteria"]["ada"],
            "Ada, engineering bot. Cuida do backend."
        );
        assert_eq!(body["state"]["bots_in_room"], json!(["Ada", "Bento"]));
        assert_eq!(body["state"]["new_message"]["text"], "quem revisa o botão?");
        assert_eq!(body["state"]["recent_messages"][0]["text"], "oi");
    }

    #[test]
    fn only_the_newest_lines_that_fit_are_sent_and_a_room_with_none_sends_no_list() {
        let long = "x".repeat(LINE_MAX);
        let recent: Vec<(String, String)> =
            (0..40).map(|i| (format!("p{i}"), long.clone())).collect();
        let kept = recent_within_budget(&recent);
        assert!(kept.len() < 40 && !kept.is_empty());
        assert_eq!(kept.last().expect("newest")["from"], "p39");

        let mut empty = question();
        empty.recent.clear();
        assert!(
            request_body(&empty).0["state"]
                .get("recent_messages")
                .is_none()
        );
    }

    #[test]
    fn an_answer_is_trusted_only_if_it_is_consistent() {
        let offered: Vec<String> = ["ada", "bento", EVERYONE].map(str::to_owned).to_vec();

        assert_eq!(
            parse_choice(&answer("ada", 0.9, 0.07, 0.03), &offered),
            Some(("ada".to_owned(), 0.9))
        );
        assert_eq!(
            parse_choice(&answer("ada", 0.91, 0.07, 0.03), &offered)
                .map(|a| a.0)
                .as_deref(),
            Some("ada"),
            "rounding to 1.01 is fine"
        );
        assert!(
            parse_choice(&answer("ada", 0.3, 0.3, 0.1), &offered).is_none(),
            "does not add up"
        );
        assert!(
            parse_choice(&answer("bento", 0.9, 0.07, 0.03), &offered).is_none(),
            "not the most likely"
        );
        assert!(
            parse_choice(&answer("zed", 0.9, 0.07, 0.03), &offered).is_none(),
            "not offered"
        );
        assert!(
            parse_choice(&answer("ada", 1.4, -0.4, 0.0), &offered).is_none(),
            "not probabilities"
        );
        assert!(parse_choice(&json!({ "answers": {} }), &offered).is_none());
        let mut wrong = answer("ada", 0.9, 0.07, 0.03);
        wrong["answers"][QUESTION]["type"] = json!("score");
        assert!(
            parse_choice(&wrong, &offered).is_none(),
            "another kind of answer"
        );
    }

    #[test]
    fn a_sure_answer_picks_and_an_unsure_one_falls_back() {
        assert_eq!(
            route_of(Some(("ada".to_owned(), 0.6))),
            Route::Member {
                id: "ada".to_owned(),
                probability: 0.6
            }
        );
        assert_eq!(
            route_of(Some((EVERYONE.to_owned(), 0.8))),
            Route::Everyone { probability: 0.8 }
        );
        assert_eq!(
            route_of(Some(("ada".to_owned(), 0.59))),
            Route::Fallback("not sure")
        );
        assert_eq!(route_of(None), Route::Fallback("malformed"));
    }

    #[test]
    fn jev_picks_over_http_with_the_key_in_the_header() {
        let (endpoint, request) = serve(
            "200 OK",
            answer("bento", 0.05, 0.93, 0.02).to_string(),
            Duration::ZERO,
        );

        let route = decide(&endpoint, "jev_secret", &question(), Duration::from_secs(5));

        assert_eq!(
            route,
            Route::Member {
                id: "bento".to_owned(),
                probability: 0.93
            }
        );
        let sent = request.recv().expect("the request").to_lowercase();
        assert!(sent.contains("authorization: bearer jev_secret"));
        assert!(sent.contains("\"jev-latest\""));
    }

    #[test]
    fn a_refusal_a_slow_answer_and_a_dead_server_all_fall_back() {
        let (endpoint, _) = serve("401 Unauthorized", "{}".to_owned(), Duration::ZERO);
        assert_eq!(
            decide(&endpoint, "k", &question(), Duration::from_secs(5)),
            Route::Fallback("key rejected")
        );

        let (endpoint, _) = serve("429 Too Many Requests", "{}".to_owned(), Duration::ZERO);
        assert_eq!(
            decide(&endpoint, "k", &question(), Duration::from_secs(5)),
            Route::Fallback("rate limited")
        );

        let (endpoint, _) = serve("200 OK", "não é json".to_owned(), Duration::ZERO);
        assert_eq!(
            decide(&endpoint, "k", &question(), Duration::from_secs(5)),
            Route::Fallback("malformed")
        );

        let (endpoint, _) = serve(
            "200 OK",
            answer("ada", 0.9, 0.07, 0.03).to_string(),
            Duration::from_millis(800),
        );
        assert_eq!(
            decide(&endpoint, "k", &question(), Duration::from_millis(100)),
            Route::Fallback("timed out")
        );

        let closed = TcpListener::bind("127.0.0.1:0").expect("a port");
        let endpoint = format!(
            "http://{}/v1/systemone",
            closed.local_addr().expect("address")
        );
        drop(closed);
        assert_eq!(
            decide(&endpoint, "k", &question(), Duration::from_secs(5)),
            Route::Fallback("unreachable")
        );
    }

    #[test]
    fn without_the_switch_or_a_key_the_decider_stays_out_of_the_way() {
        let dir = tempfile::tempdir().expect("temp dir");
        let key_file = dir.path().join("jev.key");
        let off = JevDecider::new(key_file.clone(), Arc::new(|| false));
        assert_eq!(off.choose(&question()), Route::Off);

        let on = JevDecider::new(key_file, Arc::new(|| true));
        assert_eq!(on.choose(&question()), Route::Off, "no key saved");
    }
}
