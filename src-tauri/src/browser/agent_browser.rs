//! Runs an agent's browser tools on the page the user is looking at. The agent works through the
//! same engine and the same pixels as the user: clicks and keys go in as real input, and the
//! accessibility tree gives it the page as text. Nothing here opens a DevTools port or hands the
//! agent raw DevTools access.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use lynx_browser_protocol::message::{KeyKind, MouseButton, ToHost, modifiers};
use serde_json::{Value, json};

use super::agent_permissions::{AgentBrowserAccess, Refusal, site_allowed};
use super::agent_pointer::{Controller, CursorShape, Sweep};
use super::agent_tool_definitions::Tool;
use super::browser_error::BrowserError;
use super::browser_service::BrowserService;
use super::key_names;
use super::page_snapshot::{RefTable, render};
use super::web_address::normalize;

const CALL_TIMEOUT: Duration = Duration::from_secs(10);
const LOAD_TIMEOUT: Duration = Duration::from_secs(20);
const DEFAULT_SCROLL: i64 = 600;
const MAX_WAIT_MS: u64 = 10_000;
const MAX_WAIT_TEXT_MS: u64 = 30_000;
const POLL: Duration = Duration::from_millis(250);

/// The user's settings that bound what an agent may do. Read on every call, so a change in the
/// settings applies to the next tool call.
pub trait AgentBrowserPolicy: Send + Sync + 'static {
    fn access(&self) -> AgentBrowserAccess;
    fn allowed_sites(&self) -> Vec<String>;
}

/// Where the interface hears that an agent is using the browser, so it can show the page.
pub trait AgentBrowserActivity: Send + Sync + 'static {
    fn used(&self, agent: &str, tool: &str, summary: &str);
}

/// Who is calling: the terminal session, which agent it runs, and the name the person sees.
#[derive(Debug, Clone)]
pub struct Caller {
    pub session: String,
    /// The harness name (`claude`, `codex`...), which picks the mark drawn on the pointer.
    pub harness: String,
    /// "Claude Code", "Codex"...
    pub name: String,
}

/// What a tool hands back to the agent.
#[derive(Debug, Clone, PartialEq)]
pub enum Content {
    Text(String),
    Image { base64: String, mime: &'static str },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolOutput {
    pub content: Vec<Content>,
}

impl ToolOutput {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: vec![Content::Text(text.into())],
        }
    }

    /// The MCP `tools/call` result for this output.
    pub fn to_mcp(&self, is_error: bool) -> Value {
        let content: Vec<Value> = self
            .content
            .iter()
            .map(|part| match part {
                Content::Text(text) => json!({ "type": "text", "text": text }),
                Content::Image { base64, mime } => {
                    json!({ "type": "image", "data": base64, "mimeType": mime })
                }
            })
            .collect();
        json!({ "content": content, "isError": is_error })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ToolFailure {
    #[error("unknown tool {0}")]
    UnknownTool(String),
    #[error("{0}")]
    BadArguments(String),
    #[error(transparent)]
    Refused(#[from] Refusal),
    #[error("no element has the ref {0}; take a new snapshot")]
    UnknownRef(String),
    #[error("that element is gone or has nothing to click; take a new snapshot")]
    ElementGone,
    #[error("{0}")]
    Failed(String),
    #[error(transparent)]
    Browser(#[from] BrowserError),
}

pub struct AgentBrowser {
    service: Arc<BrowserService>,
    policy: Arc<dyn AgentBrowserPolicy>,
    activity: Arc<dyn AgentBrowserActivity>,
    refs: Mutex<RefTable>,
    /// One tool call at a time: two agents sharing the page must not interleave their clicks.
    turn: Mutex<()>,
}

impl AgentBrowser {
    pub fn new(
        service: Arc<BrowserService>,
        policy: Arc<dyn AgentBrowserPolicy>,
        activity: Arc<dyn AgentBrowserActivity>,
    ) -> Self {
        Self {
            service,
            policy,
            activity,
            refs: Mutex::new(RefTable::default()),
            turn: Mutex::new(()),
        }
    }

    /// Runs one tool for the agent that is calling. Blocks while the page works, so call it off
    /// the async runtime.
    pub fn call(
        &self,
        caller: &Caller,
        name: &str,
        arguments: &Value,
    ) -> Result<ToolOutput, ToolFailure> {
        let tool =
            Tool::from_name(name).ok_or_else(|| ToolFailure::UnknownTool(name.to_owned()))?;
        match self.policy.access() {
            AgentBrowserAccess::Off => return Err(Refusal::Off.into()),
            AgentBrowserAccess::ReadOnly if tool.interacts() => {
                return Err(Refusal::ReadOnly.into());
            }
            _ => {}
        }
        let _turn = self
            .turn
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        self.service.ensure_running()?;
        let label = self.describe(tool, arguments);
        self.service.agent_begins(
            &Controller {
                session: caller.session.clone(),
                harness: caller.harness.clone(),
            },
            // Moving to an element, typing, scrolling and choosing show themselves in the pointer.
            if shows_itself(tool) { "" } else { &label },
        );
        self.activity.used(&caller.name, name, &label);

        let output = self.run(tool, arguments);
        // Whatever happened, the dots stop and the label stays as the last thing it did.
        self.service.agent_ends();
        output
    }

    fn run(&self, tool: Tool, arguments: &Value) -> Result<ToolOutput, ToolFailure> {
        Ok(match tool {
            Tool::Snapshot => {
                self.require_allowed_page()?;
                ToolOutput::text(self.snapshot()?)
            }
            Tool::Navigate => self.navigate(arguments)?,
            Tool::History => self.history(arguments)?,
            Tool::Click => self.click(arguments)?,
            Tool::Type => self.type_text(arguments)?,
            Tool::PressKey => self.press_key(arguments)?,
            Tool::Scroll => self.scroll(arguments)?,
            Tool::Wait => self.wait(arguments)?,
            Tool::Screenshot => self.screenshot()?,
            Tool::SelectOption => self.select_option(arguments)?,
        })
    }

    /// A short line for the person: what the agent is doing, naming the element by what the page
    /// calls it when that is known.
    fn describe(&self, tool: Tool, arguments: &Value) -> String {
        let detail = |key: &str| arguments.get(key).and_then(Value::as_str).unwrap_or("");
        let element = |key: &str| {
            let reference = detail(key);
            match self.refs().name_of(reference) {
                Some(name) => format!("\"{name}\""),
                None => reference.to_owned(),
            }
        };
        match tool {
            Tool::Snapshot => "Reading the page".to_owned(),
            Tool::Navigate => format!("Opening {}", short_address(detail("url"))),
            Tool::History => match detail("direction") {
                "back" => "Going back".to_owned(),
                "forward" => "Going forward".to_owned(),
                _ => "Reloading".to_owned(),
            },
            Tool::Click => format!("Clicking {}", element("ref")),
            Tool::Type if arguments.get("ref").is_some() => {
                format!("Typing in {}", element("ref"))
            }
            Tool::Type => "Typing".to_owned(),
            Tool::PressKey => format!("Pressing {}", detail("key")),
            Tool::Scroll if arguments.get("ref").is_some() => {
                format!("Scrolling to {}", element("ref"))
            }
            Tool::Scroll => format!(
                "Scrolling {}",
                arguments
                    .get("direction")
                    .and_then(Value::as_str)
                    .unwrap_or("down")
            ),
            Tool::Wait if arguments.get("text").is_some() => {
                format!("Waiting for \"{}\"", detail("text"))
            }
            Tool::Wait => "Waiting".to_owned(),
            Tool::Screenshot => "Taking a screenshot".to_owned(),
            Tool::SelectOption => format!("Choosing \"{}\"", detail("value")),
        }
    }

    fn refs(&self) -> MutexGuard<'_, RefTable> {
        self.refs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn cdp(&self, method: &str, params: Value) -> Result<Value, ToolFailure> {
        Ok(self.service.cdp(method, params, CALL_TIMEOUT)?)
    }

    /// The page the agent is on must be one it may use; a click can land on another site.
    fn require_allowed_page(&self) -> Result<(), ToolFailure> {
        let state = self.service.state();
        if site_allowed(&self.policy.allowed_sites(), &state.url) {
            return Ok(());
        }
        Err(Refusal::SiteNotAllowed(host_of(&state.url)).into())
    }

    /// After something that may have moved the page: if it landed on a site the agent may not
    /// use, the page is blanked and the agent told.
    fn leave_forbidden_site(&self) -> Result<(), ToolFailure> {
        let state = self.service.state();
        if site_allowed(&self.policy.allowed_sites(), &state.url) {
            return Ok(());
        }
        let _ = self.service.send(ToHost::Navigate {
            url: "about:blank".to_owned(),
        });
        Err(Refusal::SiteNotAllowed(host_of(&state.url)).into())
    }

    fn snapshot(&self) -> Result<String, ToolFailure> {
        let tree = self.cdp("Accessibility.getFullAXTree", json!({}))?;
        let state = self.service.state();
        let body = render(&tree, &mut self.refs());
        let mut text = format!("Page: {}\nURL: {}\n", state.title, state.url);
        if state.loading {
            text.push_str("(the page is still loading)\n");
        }
        if let Some(error) = &state.error {
            text.push_str(&format!("(the page failed to load: {error})\n"));
        }
        text.push('\n');
        text.push_str(if body.is_empty() {
            "(nothing to read yet)\n"
        } else {
            &body
        });
        Ok(text)
    }

    fn navigate(&self, arguments: &Value) -> Result<ToolOutput, ToolFailure> {
        let address = normalize(&text_argument(arguments, "url")?)?;
        if !site_allowed(&self.policy.allowed_sites(), &address) {
            return Err(Refusal::SiteNotAllowed(host_of(&address)).into());
        }
        let loads = self.service.loads_started();
        self.service.send(ToHost::Navigate { url: address })?;
        self.service.wait_until_loaded(loads, LOAD_TIMEOUT);
        self.leave_forbidden_site()?;
        Ok(ToolOutput::text(self.snapshot()?))
    }

    fn history(&self, arguments: &Value) -> Result<ToolOutput, ToolFailure> {
        self.require_allowed_page()?;
        let command = match text_argument(arguments, "direction")?.as_str() {
            "back" => ToHost::Back,
            "forward" => ToHost::Forward,
            "reload" => ToHost::Reload {
                ignore_cache: false,
            },
            other => {
                return Err(ToolFailure::BadArguments(format!(
                    "direction must be back, forward or reload, not {other}"
                )));
            }
        };
        let loads = self.service.loads_started();
        self.service.send(command)?;
        self.service.wait_until_loaded(loads, LOAD_TIMEOUT);
        self.leave_forbidden_site()?;
        Ok(ToolOutput::text(self.snapshot()?))
    }

    fn click(&self, arguments: &Value) -> Result<ToolOutput, ToolFailure> {
        self.require_allowed_page()?;
        let reference = text_argument(arguments, "ref")?;
        let node = self.node_of(&reference)?;
        let (x, y) = self.centre_of(node)?;
        let before = self.service.state();
        let loads = self.service.loads_started();

        self.service.send(ToHost::Focus { focused: true })?;
        self.glide_to(x, y);
        self.service.agent_cursor(CursorShape::Arrow);
        self.service.agent_clicks();
        self.pointer_click(x, y, 1)?;
        if arguments.get("double").and_then(Value::as_bool) == Some(true) {
            self.pointer_click(x, y, 2)?;
        }
        self.service.wait_until_loaded(loads, LOAD_TIMEOUT);
        self.leave_forbidden_site()?;

        let after = self.service.state();
        let mut text = format!("Clicked {reference}.");
        if after.url != before.url || after.title != before.title {
            text.push_str(" The page changed:\n\n");
            text.push_str(&self.snapshot()?);
        }
        Ok(ToolOutput::text(text))
    }

    /// Moves the agent's visible pointer to a point of the page and waits for it to arrive.
    fn glide_to(&self, x: i32, y: i32) {
        self.service.agent_moves_to((f64::from(x), f64::from(y)));
    }

    /// A pointer arrives before it presses, as a real one does; the page ignores a bare press.
    fn pointer_click(&self, x: i32, y: i32, clicks: u8) -> Result<(), ToolFailure> {
        self.service.send(ToHost::MouseMove {
            x,
            y,
            modifiers: 0,
            leave: false,
        })?;
        std::thread::sleep(Duration::from_millis(40));
        for pressed in [true, false] {
            self.service.send(ToHost::MouseClick {
                x,
                y,
                button: MouseButton::Left,
                pressed,
                clicks,
                modifiers: 0,
            })?;
        }
        std::thread::sleep(Duration::from_millis(60));
        Ok(())
    }

    fn type_text(&self, arguments: &Value) -> Result<ToolOutput, ToolFailure> {
        self.require_allowed_page()?;
        let text = text_argument(arguments, "text")?;
        let reference = arguments.get("ref").and_then(Value::as_str);
        self.service.send(ToHost::Focus { focused: true })?;
        if let Some(reference) = reference {
            let node = self.node_of(reference)?;
            // Show the pointer going to the field before anything is typed in it.
            if let Ok((x, y)) = self.centre_of(node) {
                self.glide_to(x, y);
                // Over a field the pointer is the text cursor.
                self.service.agent_cursor(CursorShape::IBeam);
            }
            self.cdp("DOM.focus", json!({ "backendNodeId": node }))
                .map_err(|_| ToolFailure::ElementGone)?;
            let clear = arguments.get("clear").and_then(Value::as_bool) != Some(false);
            if clear {
                self.call_on_node(node, SELECT_CONTENTS, &[])?;
            }
        }
        self.service.send(ToHost::Text { text })?;
        if arguments.get("submit").and_then(Value::as_bool) == Some(true) {
            self.press(key_names::parse("Enter").ok_or_else(unknown_key)?)?;
        }
        std::thread::sleep(Duration::from_millis(80));
        Ok(ToolOutput::text("Typed."))
    }

    fn press_key(&self, arguments: &Value) -> Result<ToolOutput, ToolFailure> {
        self.require_allowed_page()?;
        let spec = text_argument(arguments, "key")?;
        let key = key_names::parse(&spec).ok_or_else(|| {
            ToolFailure::BadArguments(format!(
                "{spec} is not a key Lynx knows; try Enter, Tab, Escape, an arrow, Home, End, PageUp, PageDown, F1-F12, a letter, or Shift/Control/Alt plus one of those"
            ))
        })?;
        self.service.send(ToHost::Focus { focused: true })?;
        let loads = self.service.loads_started();
        self.press(key)?;
        self.service.wait_until_loaded(loads, LOAD_TIMEOUT);
        self.leave_forbidden_site()?;
        Ok(ToolOutput::text(format!("Pressed {spec}.")))
    }

    fn press(&self, key: key_names::KeyPress) -> Result<(), ToolFailure> {
        // A shortcut types nothing, so a held Control or Command drops the character.
        let shortcut = key.modifiers & (modifiers::CONTROL | modifiers::COMMAND) != 0;
        let character = if shortcut { 0 } else { key.character };
        self.service.send(ToHost::Key {
            kind: KeyKind::Down,
            windows_code: key.windows_code,
            native_code: key.windows_code,
            modifiers: key.modifiers,
            character,
        })?;
        self.service.send(ToHost::Key {
            kind: KeyKind::Up,
            windows_code: key.windows_code,
            native_code: key.windows_code,
            modifiers: key.modifiers,
            character: 0,
        })?;
        std::thread::sleep(Duration::from_millis(60));
        Ok(())
    }

    fn scroll(&self, arguments: &Value) -> Result<ToolOutput, ToolFailure> {
        self.require_allowed_page()?;
        if let Some(reference) = arguments.get("ref").and_then(Value::as_str) {
            let node = self.node_of(reference)?;
            self.cdp(
                "DOM.scrollIntoViewIfNeeded",
                json!({ "backendNodeId": node }),
            )
            .map_err(|_| ToolFailure::ElementGone)?;
        } else {
            let amount = arguments
                .get("amount")
                .and_then(Value::as_i64)
                .unwrap_or(DEFAULT_SCROLL)
                .clamp(0, 20_000);
            let (dx, dy) = match arguments
                .get("direction")
                .and_then(Value::as_str)
                .unwrap_or("down")
            {
                "down" => (0, amount),
                "up" => (0, -amount),
                "right" => (amount, 0),
                "left" => (-amount, 0),
                other => {
                    return Err(ToolFailure::BadArguments(format!(
                        "direction must be down, up, left or right, not {other}"
                    )));
                }
            };
            self.service.agent_cursor(CursorShape::Arrow);
            self.service.agent_sweeps(Sweep::scrolling(dy >= 0));
            self.cdp(
                "Runtime.evaluate",
                json!({ "expression": format!("window.scrollBy({dx}, {dy})") }),
            )?;
        }
        std::thread::sleep(Duration::from_millis(120));
        let position = self.cdp(
            "Runtime.evaluate",
            json!({
                "expression": "JSON.stringify({ y: Math.round(scrollY), max: Math.max(0, Math.round(document.documentElement.scrollHeight - innerHeight)) })",
                "returnByValue": true,
            }),
        )?;
        let reading = position["result"]["value"]
            .as_str()
            .and_then(|text| serde_json::from_str::<Value>(text).ok())
            .unwrap_or(Value::Null);
        Ok(ToolOutput::text(format!(
            "Scrolled. The page is at {} of {} pixels.",
            reading["y"], reading["max"]
        )))
    }

    fn wait(&self, arguments: &Value) -> Result<ToolOutput, ToolFailure> {
        self.require_allowed_page()?;
        if let Some(text) = arguments.get("text").and_then(Value::as_str) {
            let limit = arguments
                .get("timeoutMs")
                .and_then(Value::as_u64)
                .unwrap_or(MAX_WAIT_MS)
                .min(MAX_WAIT_TEXT_MS);
            let probe = format!(
                "document.body ? document.body.innerText.includes({}) : false",
                serde_json::to_string(text).unwrap_or_default()
            );
            let started = Instant::now();
            loop {
                let found = self.cdp(
                    "Runtime.evaluate",
                    json!({ "expression": probe, "returnByValue": true }),
                )?;
                if found["result"]["value"] == json!(true) {
                    return Ok(ToolOutput::text(format!("The text appeared: {text}")));
                }
                if started.elapsed() >= Duration::from_millis(limit) {
                    return Err(ToolFailure::Failed(format!(
                        "the text did not appear within {limit} ms: {text}"
                    )));
                }
                std::thread::sleep(POLL);
            }
        }
        if let Some(ms) = arguments.get("ms").and_then(Value::as_u64) {
            std::thread::sleep(Duration::from_millis(ms.min(MAX_WAIT_MS)));
            return Ok(ToolOutput::text("Waited."));
        }
        // Nothing to wait for in particular: let a load that is under way finish.
        self.service.wait_until_idle(LOAD_TIMEOUT);
        Ok(ToolOutput::text("The page is idle."))
    }

    fn screenshot(&self) -> Result<ToolOutput, ToolFailure> {
        self.require_allowed_page()?;
        let shot = self.cdp("Page.captureScreenshot", json!({ "format": "png" }))?;
        let base64 = shot["data"]
            .as_str()
            .ok_or_else(|| ToolFailure::Failed("the page returned no picture".to_owned()))?
            .to_owned();
        let state = self.service.state();
        Ok(ToolOutput {
            content: vec![
                Content::Text(format!("{} ({})", state.title, state.url)),
                Content::Image {
                    base64,
                    mime: "image/png",
                },
            ],
        })
    }

    fn select_option(&self, arguments: &Value) -> Result<ToolOutput, ToolFailure> {
        self.require_allowed_page()?;
        let node = self.node_of(&text_argument(arguments, "ref")?)?;
        let wanted = text_argument(arguments, "value")?;
        if let Ok((x, y)) = self.centre_of(node) {
            self.glide_to(x, y);
            self.service.agent_cursor(CursorShape::Arrow);
            self.service.agent_clicks();
        }
        let chosen = self.call_on_node(node, CHOOSE_OPTION, &[json!({ "value": wanted })])?;
        match chosen.as_str() {
            Some(label) => Ok(ToolOutput::text(format!("Chose {label}."))),
            None => Err(ToolFailure::Failed(format!(
                "no option has the value or text {wanted}"
            ))),
        }
    }

    fn node_of(&self, reference: &str) -> Result<i64, ToolFailure> {
        self.refs()
            .node_of(reference)
            .ok_or_else(|| ToolFailure::UnknownRef(reference.to_owned()))
    }

    /// The middle of an element on screen, after scrolling it into view, in the view's pixels.
    fn centre_of(&self, node: i64) -> Result<(i32, i32), ToolFailure> {
        let target = json!({ "backendNodeId": node });
        self.cdp("DOM.scrollIntoViewIfNeeded", target.clone())
            .map_err(|_| ToolFailure::ElementGone)?;
        let model = self
            .cdp("DOM.getBoxModel", target)
            .map_err(|_| ToolFailure::ElementGone)?;
        let quad: Vec<f64> = model["model"]["content"]
            .as_array()
            .map(|points| points.iter().filter_map(Value::as_f64).collect())
            .unwrap_or_default();
        if quad.len() != 8 || model["model"]["width"].as_f64().unwrap_or(0.0) <= 0.0 {
            return Err(ToolFailure::ElementGone);
        }
        let x = quad.iter().step_by(2).sum::<f64>() / 4.0;
        let y = quad.iter().skip(1).step_by(2).sum::<f64>() / 4.0;
        #[allow(clippy::cast_possible_truncation)]
        Ok((x.round() as i32, y.round() as i32))
    }

    /// Calls a function with `this` set to a page node, and returns what it returned.
    fn call_on_node(
        &self,
        node: i64,
        function: &str,
        arguments: &[Value],
    ) -> Result<Value, ToolFailure> {
        let resolved = self
            .cdp("DOM.resolveNode", json!({ "backendNodeId": node }))
            .map_err(|_| ToolFailure::ElementGone)?;
        let object = resolved["object"]["objectId"]
            .as_str()
            .ok_or(ToolFailure::ElementGone)?;
        let answer = self.cdp(
            "Runtime.callFunctionOn",
            json!({
                "objectId": object,
                "functionDeclaration": function,
                "arguments": arguments,
                "returnByValue": true,
                "silent": true,
            }),
        )?;
        if let Some(details) = answer.get("exceptionDetails") {
            let message = details["exception"]["description"]
                .as_str()
                .or_else(|| details["text"].as_str())
                .unwrap_or("the page raised an error");
            return Err(ToolFailure::Failed(message.to_owned()));
        }
        Ok(answer["result"]["value"].clone())
    }
}

/// Selects what a field holds, so the next typing replaces it.
const SELECT_CONTENTS: &str = "function() { if (typeof this.select === 'function') { this.select(); } else if (this.isContentEditable) { const range = document.createRange(); range.selectNodeContents(this); const selection = getSelection(); selection.removeAllRanges(); selection.addRange(range); } }";

/// Picks the option of a `<select>` whose value or text matches, and tells the page it changed.
const CHOOSE_OPTION: &str = "function(wanted) { const option = [...this.options].find((o) => o.value === wanted || o.text.trim() === wanted); if (!option) return null; this.value = option.value; this.dispatchEvent(new Event('input', { bubbles: true })); this.dispatchEvent(new Event('change', { bubbles: true })); return option.text.trim(); }";

fn text_argument(arguments: &Value, name: &str) -> Result<String, ToolFailure> {
    arguments
        .get(name)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| ToolFailure::BadArguments(format!("{name} is required and must be text")))
}

fn unknown_key() -> ToolFailure {
    ToolFailure::Failed("that key is not known".to_owned())
}

fn host_of(address: &str) -> String {
    url::Url::parse(address)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .unwrap_or_else(|| address.to_owned())
}

/// Tools whose action the pointer already shows, so it carries no label.
fn shows_itself(tool: Tool) -> bool {
    matches!(
        tool,
        Tool::Click | Tool::Type | Tool::Scroll | Tool::SelectOption
    )
}

/// An address as a person reads it: no scheme, no trailing slash.
fn short_address(address: &str) -> String {
    let bare = address
        .strip_prefix("https://")
        .or_else(|| address.strip_prefix("http://"))
        .unwrap_or(address);
    bare.trim_end_matches('/').to_owned()
}

#[cfg(test)]
mod tests {
    use super::super::browser_service::{BrowserEvents, Frame, FrameSink};
    use super::super::browser_state::BrowserState;
    use super::*;

    struct Policy(AgentBrowserAccess, Vec<String>);

    impl AgentBrowserPolicy for Policy {
        fn access(&self) -> AgentBrowserAccess {
            self.0
        }
        fn allowed_sites(&self) -> Vec<String> {
            self.1.clone()
        }
    }

    struct Silent;

    impl AgentBrowserActivity for Silent {
        fn used(&self, _agent: &str, _tool: &str, _summary: &str) {}
    }

    struct Nowhere;

    impl BrowserEvents for Nowhere {
        fn state_changed(&self, _state: &BrowserState) {}
    }

    impl FrameSink for Nowhere {
        fn frame(&self, _frame: Frame) {}
    }

    fn caller() -> Caller {
        Caller {
            session: "s1".to_owned(),
            harness: "claude".to_owned(),
            name: "Claude Code".to_owned(),
        }
    }

    fn tools(access: AgentBrowserAccess) -> AgentBrowser {
        // No engine lives here; these tests stop before one would start.
        let service = Arc::new(BrowserService::new(
            std::env::temp_dir().join("lynx-no-engine"),
            Arc::new(Nowhere),
            Arc::new(Nowhere),
        ));
        AgentBrowser::new(
            service,
            Arc::new(Policy(access, Vec::new())),
            Arc::new(Silent),
        )
    }

    #[test]
    fn a_browser_that_is_off_refuses_every_tool() {
        let error = tools(AgentBrowserAccess::Off)
            .call(&caller(), "browser_snapshot", &json!({}))
            .unwrap_err();
        assert!(matches!(error, ToolFailure::Refused(Refusal::Off)));
    }

    #[test]
    fn read_only_refuses_what_touches_the_page_before_starting_anything() {
        let browser = tools(AgentBrowserAccess::ReadOnly);
        for tool in [
            "browser_click",
            "browser_type",
            "browser_press_key",
            "browser_select_option",
        ] {
            let error = browser.call(&caller(), tool, &json!({})).unwrap_err();
            assert!(
                matches!(error, ToolFailure::Refused(Refusal::ReadOnly)),
                "{tool}"
            );
        }
    }

    #[test]
    fn an_unknown_tool_is_named_in_the_error() {
        let error = tools(AgentBrowserAccess::Full)
            .call(&caller(), "browser_evaluate", &json!({}))
            .unwrap_err();
        assert_eq!(error.to_string(), "unknown tool browser_evaluate");
    }

    #[test]
    fn a_result_is_shaped_the_way_mcp_wants_it() {
        let output = ToolOutput {
            content: vec![
                Content::Text("hi".into()),
                Content::Image {
                    base64: "AAAA".into(),
                    mime: "image/png",
                },
            ],
        };
        assert_eq!(
            output.to_mcp(false),
            json!({
                "content": [
                    { "type": "text", "text": "hi" },
                    { "type": "image", "data": "AAAA", "mimeType": "image/png" },
                ],
                "isError": false,
            })
        );
    }

    // Needs the engine installed under the real Lynx home (`bun run browser:engine`). Run with
    // `cargo test -p lynx-ide the_tools_drive_a_real_page -- --ignored --nocapture`.
    #[test]
    #[ignore = "needs the installed browser engine"]
    fn the_tools_drive_a_real_page() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::path::PathBuf;

        const FORM: &str = "<!doctype html><title>Form</title><body style='margin:0'>\
            <h1>Sign up</h1>\
            <label>Name <input id='name' aria-label='Name'></label>\
            <select id='plan' aria-label='Plan'><option value='free'>Free</option><option value='pro'>Pro plan</option></select>\
            <button id='send' onclick=\"document.title='sent:'+document.getElementById('name').value+':'+document.getElementById('plan').value\">Send</button>\
            <a href='/second'>Next page</a>\
            <div style='height:3000px'>tall</div><p id='end'>The end</p>\
            <script>setTimeout(() => { document.body.append('Loaded late'); }, 600);\
            document.addEventListener('keydown', e => { if (e.key === 'Escape') document.title = 'escaped'; });</script>";

        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = server.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for stream in server.incoming().flatten() {
                std::thread::spawn(move || {
                    let mut stream = stream;
                    let mut request = [0u8; 2048];
                    let read = stream.read(&mut request).unwrap_or(0);
                    let request = String::from_utf8_lossy(&request[..read]);
                    let body = if request.starts_with("GET /second") {
                        "<!doctype html><title>Second</title><h1>Second page</h1>"
                    } else {
                        FORM
                    };
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                });
            }
        });

        let home = std::env::var_os("LYNX_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap()).join(".lynx"));
        let service = Arc::new(BrowserService::new(
            home,
            Arc::new(Nowhere),
            Arc::new(Nowhere),
        ));
        service.resize(800, 600, 1.0);
        let browser = AgentBrowser::new(
            Arc::clone(&service),
            Arc::new(Policy(AgentBrowserAccess::Full, Vec::new())),
            Arc::new(Silent),
        );
        let text_of = |output: &ToolOutput| match &output.content[0] {
            Content::Text(text) => text.clone(),
            Content::Image { .. } => panic!("expected text"),
        };
        let find_ref = |snapshot: &str, role: &str, name: &str| -> String {
            let line = snapshot
                .lines()
                .find(|line| line.contains(&format!("- {role} \"{name}\"")))
                .unwrap_or_else(|| panic!("no {role} {name} in:\n{snapshot}"));
            let start = line.find("[ref=").unwrap() + 5;
            line[start..].split(']').next().unwrap().to_owned()
        };

        let opened = browser
            .call(
                &caller(),
                "browser_navigate",
                &json!({ "url": format!("127.0.0.1:{port}") }),
            )
            .expect("navigate");
        let snapshot = text_of(&opened);
        println!("{snapshot}");
        assert!(snapshot.contains("Page: Form"), "{snapshot}");
        assert!(snapshot.contains("heading \"Sign up\""));

        let name = find_ref(&snapshot, "textbox", "Name");
        let send = find_ref(&snapshot, "button", "Send");
        let plan = find_ref(&snapshot, "combobox", "Plan");

        browser
            .call(
                &caller(),
                "browser_type",
                &json!({ "ref": name, "text": "Ana ção" }),
            )
            .expect("type");
        let chosen = browser
            .call(
                &caller(),
                "browser_select_option",
                &json!({ "ref": plan, "value": "Pro plan" }),
            )
            .expect("select");
        assert_eq!(text_of(&chosen), "Chose Pro plan.");
        browser
            .call(&caller(), "browser_click", &json!({ "ref": send }))
            .expect("click");
        assert_eq!(service.state().title, "sent:Ana ção:pro");

        // Typing again replaces the text instead of appending.
        browser
            .call(
                &caller(),
                "browser_type",
                &json!({ "ref": name, "text": "Bia" }),
            )
            .expect("retype");
        browser
            .call(&caller(), "browser_click", &json!({ "ref": send }))
            .expect("click again");
        assert_eq!(service.state().title, "sent:Bia:pro");

        browser
            .call(&caller(), "browser_press_key", &json!({ "key": "Escape" }))
            .expect("press");
        assert_eq!(service.state().title, "escaped");

        let scrolled = text_of(
            &browser
                .call(
                    &caller(),
                    "browser_scroll",
                    &json!({ "direction": "down", "amount": 800 }),
                )
                .expect("scroll"),
        );
        assert!(scrolled.contains("at 800 of"), "{scrolled}");

        browser
            .call(&caller(), "browser_wait", &json!({ "text": "Loaded late" }))
            .expect("wait for text");
        assert!(
            browser
                .call(
                    &caller(),
                    "browser_wait",
                    &json!({ "text": "never there", "timeoutMs": 500 })
                )
                .is_err()
        );

        // The page may not be on screen when an agent first uses it; the tools still answer.
        service.set_visible(false);
        let shot = browser
            .call(&caller(), "browser_screenshot", &json!({}))
            .expect("screenshot");
        service.set_visible(true);
        assert!(matches!(&shot.content[1], Content::Image { base64, .. } if base64.len() > 500));

        let again = text_of(
            &browser
                .call(&caller(), "browser_snapshot", &json!({}))
                .expect("snapshot"),
        );
        let link = find_ref(&again, "link", "Next page");
        let followed = text_of(
            &browser
                .call(&caller(), "browser_click", &json!({ "ref": link }))
                .expect("follow the link"),
        );
        assert!(followed.contains("Page: Second"), "{followed}");
        let back = text_of(
            &browser
                .call(
                    &caller(),
                    "browser_history",
                    &json!({ "direction": "back" }),
                )
                .expect("back"),
        );
        assert!(back.contains("heading \"Sign up\""), "{back}");

        let stale = browser
            .call(&caller(), "browser_click", &json!({ "ref": "e999" }))
            .unwrap_err();
        assert!(matches!(stale, ToolFailure::UnknownRef(_)));

        // A site off the list is refused, and a click cannot carry the agent onto one.
        let walled = AgentBrowser::new(
            Arc::clone(&service),
            Arc::new(Policy(
                AgentBrowserAccess::Full,
                vec!["example.org".to_owned()],
            )),
            Arc::new(Silent),
        );
        let refused = walled
            .call(
                &caller(),
                "browser_navigate",
                &json!({ "url": format!("127.0.0.1:{port}") }),
            )
            .unwrap_err();
        assert!(matches!(
            refused,
            ToolFailure::Refused(Refusal::SiteNotAllowed(_))
        ));
        let refused = walled
            .call(&caller(), "browser_snapshot", &json!({}))
            .unwrap_err();
        assert!(matches!(
            refused,
            ToolFailure::Refused(Refusal::SiteNotAllowed(_))
        ));

        service.shutdown();
    }
}
