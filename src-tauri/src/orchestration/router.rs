//! Sends each tool call to the handler that runs it: `bot_*` tools to the bots, `lynx_*` tools to the
//! orchestration, every other tool to the browser.

use std::sync::Arc;

use serde_json::Value;

use crate::agents::hook_hub::ToolHandler;
use crate::bots::bot_tools;

use super::service::Orchestration;
use super::tools::{self, PREFIX};

pub struct ToolRouter {
    browser: Arc<dyn ToolHandler>,
    orchestration: Arc<Orchestration>,
    /// Handles the `bot_*` tools. Absent until the bots are connected, and their calls then fall
    /// through to the browser, which answers them as unknown.
    bots: Option<Arc<dyn ToolHandler>>,
}

impl ToolRouter {
    pub fn new(browser: Arc<dyn ToolHandler>, orchestration: Arc<Orchestration>) -> Self {
        Self {
            browser,
            orchestration,
            bots: None,
        }
    }

    pub fn with_bots(mut self, bots: Arc<dyn ToolHandler>) -> Self {
        self.bots = Some(bots);
        self
    }
}

impl ToolHandler for ToolRouter {
    fn call(&self, session_id: &str, agent: &str, tool: &str, arguments: Value) -> Value {
        if let Some(bots) = self
            .bots
            .as_ref()
            .filter(|_| tool.starts_with(bot_tools::PREFIX))
        {
            bots.call(session_id, agent, tool, arguments)
        } else if tool.starts_with(PREFIX) {
            tools::call(&self.orchestration, session_id, tool, &arguments)
        } else {
            self.browser.call(session_id, agent, tool, arguments)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::{Value, json};

    use crate::agents::activity::AgentStatus;
    use crate::agents::harness::HarnessId;
    use crate::agents::hook_hub::ToolHandler;
    use crate::config::app_config::OrchestrationConfig;
    use crate::orchestration::service::{Orchestration, WorkerHost};
    use crate::persistence::AppDatabase;

    use super::ToolRouter;

    /// Orchestration on, no project, no terminals: every call is refused for lack of a project.
    struct Nothing;

    impl WorkerHost for Nothing {
        fn settings(&self) -> OrchestrationConfig {
            OrchestrationConfig {
                enabled: true,
                ..OrchestrationConfig::default()
            }
        }
        fn worktree_of(&self, _session: &str) -> Option<String> {
            None
        }
        fn new_worktree(&self, _from: &str, _name: &str) -> Result<String, String> {
            Err("no worktrees".to_owned())
        }
        fn prompt_file(&self, _text: &str) -> Result<String, String> {
            Err("no files".to_owned())
        }
        fn launch_line(&self, _harness: HarnessId, _unattended: bool) -> String {
            String::new()
        }
        fn start(&self, _worktree: &str, _command: &str) -> Result<String, String> {
            Err("no terminals".to_owned())
        }
        fn running(&self, _session: &str) -> bool {
            false
        }
        fn status(&self, _session: &str) -> Option<AgentStatus> {
            None
        }
        fn output(&self, _session: &str) -> Result<String, String> {
            Err("no terminals".to_owned())
        }
        fn stop(&self, _session: &str) {}
    }

    /// Answers every browser tool with a marker, so a test can see where a call went.
    struct Browser;

    impl ToolHandler for Browser {
        fn call(&self, _session: &str, _agent: &str, tool: &str, _arguments: Value) -> Value {
            json!({ "handledBy": "browser", "tool": tool })
        }
    }

    #[test]
    fn lynx_tools_go_to_the_orchestration_and_the_rest_to_the_browser() {
        let dir = tempfile::tempdir().expect("temp dir");
        let database = Arc::new(AppDatabase::open(&dir.path().join("app.db")).expect("database"));
        let orchestration = Arc::new(Orchestration::new(database, Arc::new(Nothing)));
        let router = ToolRouter::new(Arc::new(Browser), orchestration);

        let browser = router.call("s", "claude", "browser_snapshot", json!({}));
        assert_eq!(browser["handledBy"], "browser");

        // The session is not in a project, so the orchestration answers, and says why.
        let lynx = router.call("s", "claude", "lynx_run_start", json!({ "title": "x" }));
        assert_eq!(lynx["isError"], true);
        assert!(
            lynx["content"][0]["text"]
                .as_str()
                .expect("text")
                .contains("project")
        );
    }
}
