//! The tools an agent sees: their names, what they say about themselves, and their arguments.
//! The `--mcp` process answers `tools/list` from here without the app running, so this file has
//! to stay free of anything that needs the engine.

use serde_json::{Value, json};

/// What the agent is told about the server as a whole, once, when it connects.
pub const INSTRUCTIONS: &str = "Controls the browser pane inside Lynx. The user watches the same page live, so move at a pace they can follow. Open a page with browser_navigate, read it with browser_snapshot, and act on elements by the ref shown in the snapshot (for example e12). Refs stay valid while the element exists; take a new snapshot after the page changes.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Snapshot,
    Navigate,
    History,
    Click,
    Type,
    PressKey,
    Scroll,
    Wait,
    Screenshot,
    SelectOption,
    Console,
    Network,
}

const ALL: [Tool; 12] = [
    Tool::Snapshot,
    Tool::Navigate,
    Tool::History,
    Tool::Click,
    Tool::Type,
    Tool::PressKey,
    Tool::Scroll,
    Tool::Wait,
    Tool::Screenshot,
    Tool::SelectOption,
    Tool::Console,
    Tool::Network,
];

impl Tool {
    pub fn from_name(name: &str) -> Option<Self> {
        ALL.into_iter().find(|tool| tool.name() == name)
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Snapshot => "browser_snapshot",
            Self::Navigate => "browser_navigate",
            Self::History => "browser_history",
            Self::Click => "browser_click",
            Self::Type => "browser_type",
            Self::PressKey => "browser_press_key",
            Self::Scroll => "browser_scroll",
            Self::Wait => "browser_wait",
            Self::Screenshot => "browser_screenshot",
            Self::SelectOption => "browser_select_option",
            Self::Console => "browser_console",
            Self::Network => "browser_network",
        }
    }

    /// Tools that change what a page does, as opposed to opening and reading it. The read-only
    /// mode refuses these.
    pub fn interacts(self) -> bool {
        matches!(
            self,
            Self::Click | Self::Type | Self::PressKey | Self::SelectOption
        )
    }

    fn description(self) -> &'static str {
        match self {
            Self::Snapshot => {
                "Read the current page as text: headings, text, and the elements you can act on, each with a ref. Call this to see the page before acting."
            }
            Self::Navigate => {
                "Open a web address (http or https) and return a snapshot of the page. A bare host such as example.com is opened over https."
            }
            Self::History => {
                "Go back, go forward or reload the current page, and return a snapshot."
            }
            Self::Click => {
                "Click an element by its ref from the latest snapshot. Scrolls it into view first. If the click opens another page, the new snapshot comes back with the result."
            }
            Self::Type => {
                "Type text. With a ref, focuses that field first and replaces what it holds unless clear is false. Set submit to press Enter afterwards."
            }
            Self::PressKey => {
                "Press a key or a combination on the page, such as Enter, Tab, Escape, ArrowDown, PageDown or Shift+Tab."
            }
            Self::Scroll => {
                "Scroll the page, or bring an element into view. Returns how far down the page you are."
            }
            Self::Wait => {
                "Wait for a number of milliseconds, or until some text appears on the page. With neither, waits for the page to finish loading."
            }
            Self::Screenshot => {
                "Take a picture of what the page shows right now, the same pixels the user sees."
            }
            Self::SelectOption => {
                "Choose an option in a select element by its value or its visible label."
            }
            Self::Console => {
                "Read what the page wrote to its console since it loaded: errors, warnings and logs, with where in the code they came from. Uncaught exceptions are included. Use it to find out why a page misbehaves."
            }
            Self::Network => {
                "List the requests the page made since it loaded. By default only the ones that failed (a network error, or a status of 400 and up), with a count of the rest."
            }
        }
    }

    fn input_schema(self) -> Value {
        let element = json!({ "type": "string", "description": "The ref of an element, from browser_snapshot." });
        match self {
            Self::Snapshot | Self::Screenshot => json!({ "type": "object", "properties": {} }),
            Self::Navigate => json!({
                "type": "object",
                "properties": { "url": { "type": "string", "description": "The address to open." } },
                "required": ["url"],
            }),
            Self::History => json!({
                "type": "object",
                "properties": { "direction": { "type": "string", "enum": ["back", "forward", "reload"] } },
                "required": ["direction"],
            }),
            Self::Click => json!({
                "type": "object",
                "properties": {
                    "ref": element,
                    "double": { "type": "boolean", "description": "Double-click." },
                },
                "required": ["ref"],
            }),
            Self::Type => json!({
                "type": "object",
                "properties": {
                    "text": { "type": "string" },
                    "ref": element,
                    "clear": { "type": "boolean", "description": "Replace the field's current text. Defaults to true when a ref is given." },
                    "submit": { "type": "boolean", "description": "Press Enter after typing." },
                },
                "required": ["text"],
            }),
            Self::PressKey => json!({
                "type": "object",
                "properties": { "key": { "type": "string" } },
                "required": ["key"],
            }),
            Self::Scroll => json!({
                "type": "object",
                "properties": {
                    "direction": { "type": "string", "enum": ["down", "up", "left", "right"], "description": "Defaults to down." },
                    "amount": { "type": "integer", "description": "Pixels. Defaults to 600." },
                    "ref": element,
                },
            }),
            Self::Wait => json!({
                "type": "object",
                "properties": {
                    "ms": { "type": "integer", "description": "Milliseconds to wait, at most 10000." },
                    "text": { "type": "string", "description": "Wait until this text is on the page." },
                    "timeoutMs": { "type": "integer", "description": "Give up on the text after this long. Defaults to 10000, at most 30000." },
                },
            }),
            Self::Console => json!({
                "type": "object",
                "properties": { "level": { "type": "string", "enum": ["error", "warning", "all"], "description": "The lowest level to show. Defaults to warning, which shows warnings and errors." } },
            }),
            Self::Network => json!({
                "type": "object",
                "properties": { "all": { "type": "boolean", "description": "List every request, not only the failed ones." } },
            }),
            Self::SelectOption => json!({
                "type": "object",
                "properties": {
                    "ref": element,
                    "value": { "type": "string", "description": "The option's value or its visible text." },
                },
                "required": ["ref", "value"],
            }),
        }
    }
}

/// The `tools` array of an MCP `tools/list` answer.
pub fn list() -> Vec<Value> {
    ALL.into_iter()
        .map(|tool| {
            json!({
                "name": tool.name(),
                "description": tool.description(),
                "inputSchema": tool.input_schema(),
                "annotations": { "readOnlyHint": !tool.interacts(), "openWorldHint": true },
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_is_found_by_its_own_name() {
        for tool in ALL {
            assert_eq!(Tool::from_name(tool.name()), Some(tool));
        }
        assert_eq!(Tool::from_name("browser_evaluate"), None);
    }

    #[test]
    fn every_listed_tool_has_an_object_schema_and_a_description() {
        for entry in list() {
            assert_eq!(entry["inputSchema"]["type"], "object");
            assert!(!entry["description"].as_str().unwrap_or_default().is_empty());
        }
    }

    #[test]
    fn only_the_tools_that_touch_the_page_are_interactive() {
        let touching: Vec<_> = ALL.into_iter().filter(|tool| tool.interacts()).collect();
        assert_eq!(
            touching,
            [Tool::Click, Tool::Type, Tool::PressKey, Tool::SelectOption]
        );
    }
}
