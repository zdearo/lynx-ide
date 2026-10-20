//! What the page has said and done since it loaded: its console, the requests it made, and what
//! the engine answered on its own (dialogs, refused permissions). Agents read it to see why a page
//! misbehaves, which is what testing an app in the browser is for. It keeps the latest few hundred
//! entries and starts over when the main frame navigates, like DevTools without "preserve log".

use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, MutexGuard, PoisonError};

use serde_json::Value;

const MAX_CONSOLE: usize = 300;
const MAX_REQUESTS: usize = 400;
const MAX_TEXT_CHARS: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Debug,
    Info,
    Warning,
    Error,
}

impl Level {
    fn name(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct ConsoleEntry {
    level: Level,
    text: String,
    /// Where in the page's code it came from, as `url:line`.
    origin: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
struct Request {
    method: String,
    url: String,
    resource_type: String,
    status: Option<u16>,
    failure: Option<String>,
}

#[derive(Default)]
struct State {
    console: VecDeque<ConsoleEntry>,
    requests: HashMap<String, Request>,
    /// Request ids in the order they began, oldest first.
    order: VecDeque<String>,
    /// Said by the engine, not yet shown to the agent.
    notices: Vec<String>,
}

#[derive(Default)]
pub struct PageLog {
    state: Mutex<State>,
}

impl PageLog {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Folds in one DevTools event. Anything it does not know is ignored.
    pub fn apply_event(&self, method: &str, params: &Value) {
        let mut state = self.lock();
        match method {
            "Runtime.consoleAPICalled" => state.push_console(console_call(params)),
            "Runtime.exceptionThrown" => state.push_console(exception(params)),
            "Log.entryAdded" => state.push_console(log_entry(params)),
            "Network.requestWillBeSent" => state.begin_request(params),
            "Network.responseReceived" => state.finish_request(params),
            "Network.loadingFailed" => state.fail_request(params),
            // A new page: what the old one said is not about this one.
            "Page.frameNavigated" if params["frame"].get("parentId").is_none() => {
                state.console.clear();
                state.requests.clear();
                state.order.clear();
            }
            _ => {}
        }
    }

    /// The engine answered something by itself. The person sees it in the console too, and the
    /// agent is told once, with its next result.
    pub fn add_notice(&self, text: &str) {
        let mut state = self.lock();
        state.push_console(Some(ConsoleEntry {
            level: Level::Warning,
            text: format!("Lynx: {text}"),
            origin: None,
        }));
        state.notices.push(text.to_owned());
    }

    /// What the engine did on its own since the agent last looked, once.
    pub fn take_notices(&self) -> Vec<String> {
        std::mem::take(&mut self.lock().notices)
    }

    /// The console as text: the latest entries at or above `minimum`, newest last.
    pub fn console_text(&self, minimum: Level, limit: usize) -> String {
        let state = self.lock();
        let shown: Vec<&ConsoleEntry> = state
            .console
            .iter()
            .filter(|entry| entry.level >= minimum)
            .collect();
        if shown.is_empty() {
            return format!(
                "The console has nothing at {} or above since the page loaded ({} entries in all).\n",
                minimum.name(),
                state.console.len()
            );
        }
        let skipped = shown.len().saturating_sub(limit);
        let mut text = format!(
            "Console since the page loaded ({} of {} entries):\n",
            shown.len() - skipped,
            state.console.len()
        );
        for entry in shown.into_iter().skip(skipped) {
            text.push_str(&format!("[{}] {}", entry.level.name(), entry.text));
            if let Some(origin) = &entry.origin {
                text.push_str(&format!(" ({origin})"));
            }
            text.push('\n');
        }
        text
    }

    /// The requests as text: the failed ones (a network error or a status of 400 and up), or all
    /// of them, the latest `limit`, oldest first.
    pub fn network_text(&self, all: bool, limit: usize) -> String {
        let state = self.lock();
        let requests: Vec<&Request> = state
            .order
            .iter()
            .filter_map(|id| state.requests.get(id))
            .collect();
        let shown: Vec<&&Request> = requests
            .iter()
            .filter(|request| all || request.is_failure())
            .collect();
        if shown.is_empty() {
            return format!(
                "No request has failed since the page loaded ({} requests in all).\n",
                requests.len()
            );
        }
        let skipped = shown.len().saturating_sub(limit);
        let mut text = format!(
            "{} of {} requests since the page loaded:\n",
            shown.len() - skipped,
            requests.len()
        );
        for request in shown.into_iter().skip(skipped) {
            let outcome = match (&request.failure, request.status) {
                (Some(error), _) => format!("FAILED {error}"),
                (None, Some(status)) => status.to_string(),
                (None, None) => "pending".to_owned(),
            };
            text.push_str(&format!(
                "{outcome} {} {} ({})\n",
                request.method,
                clip(&request.url, MAX_TEXT_CHARS),
                request.resource_type
            ));
        }
        text
    }
}

impl Request {
    fn is_failure(&self) -> bool {
        self.failure.is_some() || self.status.is_some_and(|status| status >= 400)
    }
}

impl State {
    fn push_console(&mut self, entry: Option<ConsoleEntry>) {
        let Some(entry) = entry else { return };
        if self.console.len() == MAX_CONSOLE {
            self.console.pop_front();
        }
        self.console.push_back(entry);
    }

    fn begin_request(&mut self, params: &Value) {
        let Some(id) = params["requestId"].as_str() else {
            return;
        };
        let request = Request {
            method: params["request"]["method"]
                .as_str()
                .unwrap_or("GET")
                .to_owned(),
            url: params["request"]["url"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            resource_type: params["type"].as_str().unwrap_or("other").to_lowercase(),
            status: None,
            failure: None,
        };
        // A redirect reuses the id: the new hop replaces the old one in place.
        if self.requests.insert(id.to_owned(), request).is_none() {
            if self.order.len() == MAX_REQUESTS
                && let Some(oldest) = self.order.pop_front()
            {
                self.requests.remove(&oldest);
            }
            self.order.push_back(id.to_owned());
        }
    }

    fn finish_request(&mut self, params: &Value) {
        let Some(request) = params["requestId"]
            .as_str()
            .and_then(|id| self.requests.get_mut(id))
        else {
            return;
        };
        request.status = params["response"]["status"]
            .as_u64()
            .and_then(|status| u16::try_from(status).ok());
    }

    fn fail_request(&mut self, params: &Value) {
        let Some(request) = params["requestId"]
            .as_str()
            .and_then(|id| self.requests.get_mut(id))
        else {
            return;
        };
        // A request the page cancelled itself (a navigation, an aborted fetch) is not a failure.
        if params["canceled"].as_bool() == Some(true) {
            request.status.get_or_insert(0);
            return;
        }
        request.failure = Some(
            params["errorText"]
                .as_str()
                .unwrap_or("the request failed")
                .to_owned(),
        );
    }
}

fn console_call(params: &Value) -> Option<ConsoleEntry> {
    let level = match params["type"].as_str()? {
        "error" | "assert" => Level::Error,
        "warning" => Level::Warning,
        "debug" => Level::Debug,
        "log" | "info" => Level::Info,
        // Tables, groups, timers and the like are page chatter.
        _ => return None,
    };
    let text = params["args"]
        .as_array()?
        .iter()
        .map(remote_object_text)
        .collect::<Vec<_>>()
        .join(" ");
    let frame = &params["stackTrace"]["callFrames"][0];
    Some(ConsoleEntry {
        level,
        text: clip(&text, MAX_TEXT_CHARS),
        origin: origin_of(frame["url"].as_str(), frame["lineNumber"].as_u64()),
    })
}

fn exception(params: &Value) -> Option<ConsoleEntry> {
    let details = &params["exceptionDetails"];
    // The description of the error object has its message and stack; the bare text is "Uncaught".
    let text = details["exception"]["description"]
        .as_str()
        .or_else(|| details["text"].as_str())?;
    let first_lines: Vec<&str> = text.lines().take(4).collect();
    Some(ConsoleEntry {
        level: Level::Error,
        text: clip(
            &format!("Uncaught {}", first_lines.join(" | ")),
            MAX_TEXT_CHARS,
        ),
        origin: origin_of(details["url"].as_str(), details["lineNumber"].as_u64()),
    })
}

fn log_entry(params: &Value) -> Option<ConsoleEntry> {
    let entry = &params["entry"];
    let level = match entry["level"].as_str()? {
        "error" => Level::Error,
        "warning" => Level::Warning,
        "info" => Level::Info,
        _ => Level::Debug,
    };
    Some(ConsoleEntry {
        level,
        text: clip(entry["text"].as_str()?, MAX_TEXT_CHARS),
        origin: origin_of(entry["url"].as_str(), entry["lineNumber"].as_u64()),
    })
}

/// How a value passed to `console.*` reads: strings and numbers as they are, objects by their
/// description.
fn remote_object_text(object: &Value) -> String {
    match &object["value"] {
        Value::String(text) => text.clone(),
        Value::Null if object.get("value").is_none() => object["description"]
            .as_str()
            .or_else(|| object["type"].as_str())
            .unwrap_or("?")
            .to_owned(),
        other => other.to_string(),
    }
}

fn origin_of(url: Option<&str>, line: Option<u64>) -> Option<String> {
    let url = url.filter(|url| !url.is_empty())?;
    Some(match line {
        Some(line) => format!("{url}:{}", line + 1),
        None => url.to_owned(),
    })
}

fn clip(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    let cut: String = text.chars().take(limit).collect();
    format!("{cut}…")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn console(log: &PageLog, kind: &str, args: Value) {
        log.apply_event(
            "Runtime.consoleAPICalled",
            &json!({
                "type": kind,
                "args": args,
                "stackTrace": { "callFrames": [{ "url": "http://app.test/main.js", "lineNumber": 9 }] },
            }),
        );
    }

    #[test]
    fn console_calls_are_read_with_their_values_and_origin() {
        let log = PageLog::default();
        console(
            &log,
            "error",
            json!([{ "type": "string", "value": "boom:" }, { "type": "number", "value": 42 }]),
        );
        console(
            &log,
            "log",
            json!([{ "type": "object", "description": "Object" }]),
        );

        let errors = log.console_text(Level::Warning, 50);
        assert!(
            errors.contains("[error] boom: 42 (http://app.test/main.js:10)"),
            "{errors}"
        );
        assert!(!errors.contains("Object"));
        assert!(log.console_text(Level::Debug, 50).contains("[info] Object"));
    }

    #[test]
    fn an_uncaught_exception_shows_its_message_and_where_it_happened() {
        let log = PageLog::default();
        log.apply_event(
            "Runtime.exceptionThrown",
            &json!({ "exceptionDetails": {
                "text": "Uncaught",
                "url": "http://app.test/app.js",
                "lineNumber": 3,
                "exception": { "description": "TypeError: x is not a function\n    at run (app.js:4:1)" },
            } }),
        );

        let text = log.console_text(Level::Error, 50);
        assert!(
            text.contains("Uncaught TypeError: x is not a function |     at run"),
            "{text}"
        );
        assert!(text.contains("(http://app.test/app.js:4)"));
    }

    #[test]
    fn the_browsers_own_messages_are_in_the_console_too() {
        let log = PageLog::default();
        log.apply_event(
            "Log.entryAdded",
            &json!({ "entry": { "level": "error", "text": "Failed to load resource: 404", "url": "http://app.test/x.png" } }),
        );

        assert!(
            log.console_text(Level::Error, 50)
                .contains("Failed to load resource: 404")
        );
    }

    #[test]
    fn failed_requests_are_listed_and_the_rest_only_counted() {
        let log = PageLog::default();
        let begin = |id: &str, url: &str, kind: &str| {
            log.apply_event(
                "Network.requestWillBeSent",
                &json!({ "requestId": id, "type": kind, "request": { "url": url, "method": "GET" } }),
            );
        };
        begin("1", "http://app.test/", "Document");
        begin("2", "http://app.test/missing.js", "Script");
        begin("3", "http://api.test/data", "Fetch");
        log.apply_event(
            "Network.responseReceived",
            &json!({ "requestId": "1", "response": { "status": 200 } }),
        );
        log.apply_event(
            "Network.responseReceived",
            &json!({ "requestId": "2", "response": { "status": 404 } }),
        );
        log.apply_event(
            "Network.loadingFailed",
            &json!({ "requestId": "3", "errorText": "net::ERR_CONNECTION_REFUSED" }),
        );

        let failed = log.network_text(false, 50);
        assert!(failed.starts_with("2 of 3 requests"), "{failed}");
        assert!(failed.contains("404 GET http://app.test/missing.js (script)"));
        assert!(
            failed.contains("FAILED net::ERR_CONNECTION_REFUSED GET http://api.test/data (fetch)")
        );
        assert!(!failed.contains("http://app.test/ (document)"));
        assert!(
            log.network_text(true, 50)
                .contains("200 GET http://app.test/ (document)")
        );
    }

    #[test]
    fn a_request_the_page_cancelled_is_not_a_failure() {
        let log = PageLog::default();
        log.apply_event(
            "Network.requestWillBeSent",
            &json!({ "requestId": "1", "request": { "url": "http://app.test/a", "method": "GET" } }),
        );
        log.apply_event(
            "Network.loadingFailed",
            &json!({ "requestId": "1", "errorText": "net::ERR_ABORTED", "canceled": true }),
        );

        assert!(
            log.network_text(false, 50)
                .starts_with("No request has failed")
        );
    }

    #[test]
    fn navigating_the_main_frame_starts_the_log_over_but_a_subframe_does_not() {
        let log = PageLog::default();
        console(&log, "error", json!([{ "type": "string", "value": "old" }]));

        log.apply_event(
            "Page.frameNavigated",
            &json!({ "frame": { "id": "child", "parentId": "main", "url": "http://x/" } }),
        );
        assert!(log.console_text(Level::Error, 50).contains("old"));

        log.apply_event(
            "Page.frameNavigated",
            &json!({ "frame": { "id": "main", "url": "http://x/" } }),
        );
        assert!(!log.console_text(Level::Debug, 50).contains("old"));
    }

    #[test]
    fn only_the_latest_entries_are_kept_and_shown() {
        let log = PageLog::default();
        for index in 0..(MAX_CONSOLE + 20) {
            console(
                &log,
                "error",
                json!([{ "type": "string", "value": format!("e{index}") }]),
            );
        }

        let text = log.console_text(Level::Error, 5);
        assert!(text.contains(&format!("e{}", MAX_CONSOLE + 19)));
        assert!(!text.contains("e0 "), "the oldest are gone");
        assert_eq!(text.lines().count(), 6, "a header and five entries");
    }

    #[test]
    fn what_the_engine_did_alone_is_told_once_and_stays_in_the_console() {
        let log = PageLog::default();
        log.add_notice("The page showed a confirm \"Delete?\"; Lynx answered no.");

        assert_eq!(log.take_notices().len(), 1);
        assert!(log.take_notices().is_empty());
        assert!(
            log.console_text(Level::Warning, 50)
                .contains("Lynx: The page showed a confirm")
        );
    }
}
