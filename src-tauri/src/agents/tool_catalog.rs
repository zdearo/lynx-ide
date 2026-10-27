//! Every tool an agent can call through the MCP server: the browser's and the orchestration's. A bot
//! is offered its own tools instead.

use serde_json::Value;

use crate::bots::bot_tools;
use crate::browser::agent_tool_definitions;
use crate::orchestration::tools;

pub fn list() -> Vec<Value> {
    let mut all = agent_tool_definitions::list();
    all.extend(tools::definitions());
    all
}

/// The tools a session is offered. A bot's session (`bot:<id>`) gets the bot tools, and an agent gets
/// `list`.
pub fn list_for(caller: Option<&str>) -> Vec<Value> {
    match caller {
        Some(session) if session.starts_with("bot:") => bot_tools::definitions(),
        _ => list(),
    }
}

pub fn instructions() -> String {
    format!(
        "{} {}",
        agent_tool_definitions::INSTRUCTIONS,
        tools::INSTRUCTIONS
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_agent_is_offered_the_browser_tools_and_the_orchestration_tools() {
        let names: Vec<String> = list()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap_or_default().to_owned())
            .collect();
        assert!(names.iter().any(|name| name == "browser_snapshot"));
        assert!(names.iter().any(|name| name == "lynx_run_start"));
        assert!(instructions().contains("lynx_worker_read"));
        assert!(instructions().contains("browser_snapshot"));
    }
}
