//! What lets a bot's Claude reach Lynx: the app's MCP server, registered on the hub for the length of
//! one turn, and the folders of the repositories the bot was granted. A bot is never a terminal, so
//! its session exists only while it answers.

use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};

use crate::agents::agent_monitor::AgentInput;
use crate::agents::hook_hub::HookHub;

use super::bot_access::RegisteredRepos;

/// The name of the bot's MCP server. Its tools are named `mcp__lynx-bot__<tool>`, apart from the
/// agents' `lynx` server, so the bot's permissions and the logs say which one a call came from.
pub const SERVER_NAME: &str = "lynx-bot";

pub struct BotBridge {
    hub: HookHub,
    /// The app's own executable, started in MCP mode for the bot.
    executable: PathBuf,
    repos: Arc<dyn RegisteredRepos>,
}

impl BotBridge {
    pub fn new(hub: HookHub, executable: PathBuf, repos: Arc<dyn RegisteredRepos>) -> Self {
        Self {
            hub,
            executable,
            repos,
        }
    }

    pub fn repos(&self) -> &dyn RegisteredRepos {
        self.repos.as_ref()
    }

    /// Opens a session of a bot on the hub. The hub answers the bot's tool calls only while the
    /// returned guard lives, so the session ends with the turn. Each turn has its own session, so two
    /// threads of a bot can answer at once and each call is told from the other.
    pub fn open_session(&self, session: &str) -> SessionGuard {
        let (sender, _receiver) = mpsc::channel::<AgentInput>();
        self.hub.register(session, sender);
        SessionGuard {
            hub: self.hub.clone(),
            session: session.to_owned(),
        }
    }

    /// The app's executable, which a bot's shell runs with `--cli` to reach Lynx's tools.
    pub fn executable(&self) -> &Path {
        &self.executable
    }

    /// The environment the hub gives a turn's session: what the CLI reads to reach the app as that session.
    pub fn session_env(&self, session: &str) -> Vec<(String, String)> {
        self.hub.session_env(session)
    }
}

/// Ends a bot's hub session when it goes out of scope.
pub struct SessionGuard {
    hub: HookHub,
    session: String,
}

impl Drop for SessionGuard {
    fn drop(&mut self) {
        self.hub.unregister(&self.session);
    }
}

/// The prefix of a bot's session id, which the hub, the tools and the orchestration read the bot back from.
pub const BOT_SESSION_PREFIX: &str = "bot:";

/// The session id of one turn of a bot on the hub: `bot:<bot>:<label>`. The label is the id of the
/// thread the bot answers in, or `ch-<channel>` when it answers in a channel.
pub fn session_of(bot: &str, label: &str) -> String {
    format!("{BOT_SESSION_PREFIX}{bot}:{label}")
}

/// The bot and the label of a session, or None when it is not a bot's.
pub fn parse_session(session: &str) -> Option<(&str, &str)> {
    let (bot, label) = session.strip_prefix(BOT_SESSION_PREFIX)?.split_once(':')?;
    (!bot.is_empty() && !label.is_empty()).then_some((bot, label))
}

/// The bot a session belongs to.
pub fn bot_of_session(session: &str) -> Option<&str> {
    parse_session(session).map(|(bot, _)| bot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::hook_hub::SESSION_ENV;
    use crate::bots::bot_access::RepoRef;

    /// No repositories: only the MCP config is under test.
    struct Nothing;

    impl RegisteredRepos for Nothing {
        fn repos(&self) -> Vec<RepoRef> {
            Vec::new()
        }

        fn folders_of(&self, _repo_id: &str) -> Vec<PathBuf> {
            Vec::new()
        }
    }

    #[test]
    fn a_turns_environment_names_its_session_so_the_cli_acts_as_it() {
        let bridge = BotBridge::new(
            HookHub::start(),
            PathBuf::from("/opt/lynx/lynx"),
            Arc::new(Nothing),
        );

        let env: std::collections::HashMap<String, String> = bridge
            .session_env(&session_of("ada", "t1"))
            .into_iter()
            .collect();

        assert_eq!(env.get(SESSION_ENV).map(String::as_str), Some("bot:ada:t1"));
        assert_eq!(bridge.executable(), Path::new("/opt/lynx/lynx"));
    }

    #[test]
    fn a_session_names_its_bot_and_its_label_and_nothing_else_parses() {
        let session = session_of("ada", "ch-dev");
        assert_eq!(parse_session(&session), Some(("ada", "ch-dev")));
        assert_eq!(bot_of_session(&session), Some("ada"));
        for other in ["bot:ada", "bot::t1", "bot:ada:", "ada:t1", "agent-1"] {
            assert_eq!(parse_session(other), None, "{other}");
        }
    }
}
