//! Connects the agent tools to the rest of the app: the user's settings bound them, the hook hub
//! carries the calls in, and the interface hears what the agent is doing.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;
use tauri::AppHandle;
use tauri_specta::Event;

use lynx_browser_protocol::message::PageId;

use crate::agents::hook_hub::ToolHandler;
use crate::config::ConfigStore;

use super::agent_browser::{
    AgentBrowser, AgentBrowserActivity, AgentBrowserPolicy, Caller, ToolOutput,
};
use super::agent_permissions::AgentBrowserAccess;
use super::browser_service::BrowserService;
use super::page_owner::SessionOwners;

/// Sent when an agent uses the browser, so the interface can show the page it is working on.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct BrowserAgentActivity {
    pub tool: String,
    /// A short line for the interface, such as "Opening example.com".
    pub summary: String,
    /// Who is acting, as the person knows it: "Claude Code", "Codex"...
    pub agent: String,
    /// The page the agent acts on, so the interface shows the action only for the page it shows.
    pub page: u32,
}

/// Sent when no agent is in control of the page any more.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct BrowserAgentReleased {}

/// Reads the limits from the settings on every call, so a change applies to the next tool call.
pub struct ConfigPolicy {
    config: Arc<ConfigStore>,
}

impl AgentBrowserPolicy for ConfigPolicy {
    fn access(&self) -> AgentBrowserAccess {
        self.config.config().browser.agent_access
    }

    fn allowed_sites(&self) -> Vec<String> {
        self.config.config().browser.allowed_sites
    }
}

struct TauriActivity {
    app: AppHandle,
}

impl AgentBrowserActivity for TauriActivity {
    fn used(&self, agent: &str, page: PageId, tool: &str, summary: &str) {
        // A failed emit means the window is closing; there is nobody left to show it to.
        let _ = BrowserAgentActivity {
            page,
            tool: tool.to_owned(),
            summary: summary.to_owned(),
            agent: agent.to_owned(),
        }
        .emit(&self.app);
    }
}

/// What the hook hub calls for a tool an agent asked for.
pub struct AgentToolBridge {
    browser: AgentBrowser,
    owners: SessionOwners,
}

impl AgentToolBridge {
    pub fn new(
        app: AppHandle,
        service: Arc<BrowserService>,
        config: Arc<ConfigStore>,
        owners: SessionOwners,
    ) -> Self {
        Self {
            browser: AgentBrowser::new(
                service,
                Arc::new(ConfigPolicy { config }),
                Arc::new(TauriActivity { app }),
            ),
            owners,
        }
    }
}

impl ToolHandler for AgentToolBridge {
    fn call(&self, session_id: &str, agent: &str, tool: &str, arguments: Value) -> Value {
        // A session without an owner was not started from a Lynx terminal, so it has no pages.
        let Some(owner) = self.owners.get(session_id) else {
            return ToolOutput::text(
                "this agent's session has no browser owner; start the agent from a Lynx terminal",
            )
            .to_mcp(true);
        };
        let caller = Caller {
            session: session_id.to_owned(),
            harness: agent.to_owned(),
            name: agent_name(agent),
        };
        match self.browser.call(&caller, &owner, tool, &arguments) {
            Ok(output) => output.to_mcp(false),
            Err(failure) => ToolOutput::text(failure.to_string()).to_mcp(true),
        }
    }
}

/// The name the person knows an agent by, from the harness name its MCP entry carries.
fn agent_name(harness: &str) -> String {
    match harness {
        "claude" => "Claude Code",
        "codex" => "Codex",
        "opencode" => "OpenCode",
        _ => "Agent",
    }
    .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agents_are_named_the_way_the_settings_name_them() {
        assert_eq!(agent_name("claude"), "Claude Code");
        assert_eq!(agent_name("opencode"), "OpenCode");
        assert_eq!(agent_name(""), "Agent");
    }
}
