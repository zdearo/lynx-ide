//! Takes out the `lynx` MCP server that earlier builds added to each agent's own configuration. The tools
//! now come through the agent's shell (`lynx-ide --cli`), so nothing is added any more. Only the `lynx`
//! entry is ever touched, and the rest of each file stays as it was.
//!
//! Claude Code keeps its servers in a file it rewrites constantly while it runs, so Lynx asks the
//! `claude` CLI to remove the entry. Codex and OpenCode keep theirs in plain config files, edited here.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde_json::{Map, Value};
use toml_edit::{DocumentMut, Item};

use crate::process::bounded_command::{BoundedError, run_bounded};

use super::harness::HarnessId;
use super::hook_install::{
    HookInstallError, invalid_config, io_error, read_optional, write_atomic,
};

/// The name the server had in every agent's list.
pub const SERVER_NAME: &str = "lynx";
const CLI_TIMEOUT: Duration = Duration::from_secs(20);

/// Where each agent kept its MCP servers.
#[derive(Clone)]
pub struct McpCleanup {
    claude_config: PathBuf,
    codex_config: PathBuf,
    opencode_config: PathBuf,
}

impl McpCleanup {
    pub fn new(claude_config: PathBuf, codex_config: PathBuf, opencode_config: PathBuf) -> Self {
        Self {
            claude_config,
            codex_config,
            opencode_config,
        }
    }

    /// Removes the old entry when the agent has one. An agent without one is left alone, and so is an agent
    /// whose config file cannot be read, which the error says.
    pub fn remove_if_present(&self, harness: HarnessId) -> Result<(), HookInstallError> {
        if !self.is_present(harness)? {
            return Ok(());
        }
        self.remove(harness)
    }

    /// Whether the agent still has a `lynx` server, whichever program it points at.
    pub fn is_present(&self, harness: HarnessId) -> Result<bool, HookInstallError> {
        Ok(match harness {
            HarnessId::Claude => claude_entry(&self.claude_config)?.is_some(),
            HarnessId::Codex => codex_document(&self.codex_config)?
                .get("mcp_servers")
                .and_then(Item::as_table_like)
                .is_some_and(|servers| servers.contains_key(SERVER_NAME)),
            HarnessId::Opencode => opencode_root(&self.opencode_config)?
                .get("mcp")
                .and_then(|servers| servers.get(SERVER_NAME))
                .is_some(),
            HarnessId::Omp => false,
        })
    }

    fn remove(&self, harness: HarnessId) -> Result<(), HookInstallError> {
        match harness {
            HarnessId::Claude => self.remove_claude(),
            HarnessId::Codex => self.remove_codex(),
            HarnessId::Opencode => self.remove_opencode(),
            HarnessId::Omp => Ok(()),
        }
    }

    /// The folder the CLI must use: only in a debug build, where `claude_config` is the debug folder's file.
    fn claude_cli_dir(&self) -> Option<&Path> {
        if cfg!(debug_assertions) {
            self.claude_config.parent()
        } else {
            None
        }
    }

    fn remove_claude(&self) -> Result<(), HookInstallError> {
        if claude_entry(&self.claude_config)?.is_none() {
            return Ok(());
        }
        claude_cli(
            &["mcp", "remove", "--scope", "user", SERVER_NAME],
            self.claude_cli_dir(),
        )
    }

    fn remove_codex(&self) -> Result<(), HookInstallError> {
        let path = &self.codex_config;
        let mut document = codex_document(path)?;
        let Some(servers) = document
            .get_mut("mcp_servers")
            .and_then(Item::as_table_like_mut)
        else {
            return Ok(());
        };
        if servers.remove(SERVER_NAME).is_none() {
            return Ok(());
        }
        if servers.is_empty() {
            document.remove("mcp_servers");
        }
        write_atomic(path, &document.to_string())
    }

    fn remove_opencode(&self) -> Result<(), HookInstallError> {
        let path = &self.opencode_config;
        let mut root = opencode_root(path)?;
        let Some(servers) = root.get_mut("mcp").and_then(Value::as_object_mut) else {
            return Ok(());
        };
        if servers.shift_remove(SERVER_NAME).is_none() {
            return Ok(());
        }
        if servers.is_empty() {
            root.shift_remove("mcp");
        }
        write_json(path, &root)
    }
}

fn write_json(path: &Path, root: &Map<String, Value>) -> Result<(), HookInstallError> {
    let mut text = serde_json::to_string_pretty(root).map_err(|_| invalid_config(path))?;
    text.push('\n');
    write_atomic(path, &text)
}

/// Codex's `config.toml`; a missing file is an empty one. A file that is not TOML is left alone.
fn codex_document(path: &Path) -> Result<DocumentMut, HookInstallError> {
    let Some(text) = read_optional(path)? else {
        return Ok(DocumentMut::new());
    };
    text.parse().map_err(|_| invalid_config(path))
}

/// OpenCode's `opencode.json` as an object; a missing or empty file is an empty one.
fn opencode_root(path: &Path) -> Result<Map<String, Value>, HookInstallError> {
    let Some(text) = read_optional(path)? else {
        return Ok(Map::new());
    };
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(root)) => Ok(root),
        _ => Err(invalid_config(path)),
    }
}

/// The `lynx` entry in Claude's user-level server list, read straight from its file.
fn claude_entry(path: &Path) -> Result<Option<Value>, HookInstallError> {
    let Some(text) = read_optional(path)? else {
        return Ok(None);
    };
    let Ok(root) = serde_json::from_str::<Value>(&text) else {
        return Err(invalid_config(path));
    };
    Ok(root
        .get("mcpServers")
        .and_then(|servers| servers.get(SERVER_NAME))
        .cloned())
}

/// Runs `claude` with these arguments. A change the CLI refuses is reported with what it said.
fn claude_cli(arguments: &[&str], config_dir: Option<&Path>) -> Result<(), HookInstallError> {
    let mut command = Command::new("claude");
    command.args(arguments);
    // A debug build points the CLI at its own folder, or it would edit the installed app's MCP entry.
    if let Some(dir) = config_dir {
        command.env("CLAUDE_CONFIG_DIR", dir);
    }
    let shown = format!("claude {}", arguments.join(" "));
    let output = run_bounded(&mut command, b"", CLI_TIMEOUT).map_err(|error| match error {
        BoundedError::Spawn(source) => io_error(Path::new("claude"), source),
        BoundedError::TimedOut => HookInstallError::Command {
            command: shown.clone(),
            detail: "it did not finish in time".to_owned(),
        },
    })?;
    if output.status.success() {
        return Ok(());
    }
    let said = String::from_utf8_lossy(&output.stderr);
    let said = said.trim();
    Err(HookInstallError::Command {
        command: shown,
        detail: if said.is_empty() {
            format!("it exited with {}", output.status)
        } else {
            said.to_owned()
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cleanup(dir: &Path) -> McpCleanup {
        McpCleanup::new(
            dir.join(".claude.json"),
            dir.join("config.toml"),
            dir.join("opencode.json"),
        )
    }

    #[test]
    fn codex_removal_takes_only_our_entry_and_tidies_an_empty_list() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("config.toml");
        std::fs::write(
            &file,
            "model = \"gpt\"\n\n[mcp_servers.lynx]\ncommand = \"/x\"\n\n[mcp_servers.other]\ncommand = \"/y\"\n",
        )
        .expect("write");

        cleanup(dir.path())
            .remove_if_present(HarnessId::Codex)
            .expect("remove");

        let text = std::fs::read_to_string(&file).expect("read");
        assert!(text.contains("model = \"gpt\""), "{text}");
        assert!(!text.contains("[mcp_servers.lynx]"), "{text}");
        assert!(text.contains("[mcp_servers.other]"), "{text}");
    }

    #[test]
    fn an_agent_without_the_old_entry_is_left_untouched() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("opencode.json");
        std::fs::write(&file, "{\"model\": \"x\"}\n").expect("write");

        cleanup(dir.path())
            .remove_if_present(HarnessId::Opencode)
            .expect("nothing to remove");

        assert_eq!(
            std::fs::read_to_string(&file).expect("read"),
            "{\"model\": \"x\"}\n"
        );
    }

    #[test]
    fn opencode_removal_takes_only_our_entry_and_keeps_its_other_settings() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("opencode.json");
        std::fs::write(
            &file,
            r#"{"model": "x", "mcp": {"lynx": {"type": "local"}, "other": {"type": "local"}}}"#,
        )
        .expect("write");

        cleanup(dir.path())
            .remove_if_present(HarnessId::Opencode)
            .expect("remove");

        let root: Value =
            serde_json::from_str(&std::fs::read_to_string(&file).expect("read")).expect("json");
        assert_eq!(root["model"], "x");
        assert!(root["mcp"].get("lynx").is_none());
        assert!(root["mcp"].get("other").is_some());
    }

    #[test]
    fn a_config_that_is_not_toml_is_reported_and_never_overwritten() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("config.toml");
        std::fs::write(&file, "isto não é toml = = =").expect("write");

        assert!(
            cleanup(dir.path())
                .remove_if_present(HarnessId::Codex)
                .is_err()
        );
        assert_eq!(
            std::fs::read_to_string(&file).expect("read"),
            "isto não é toml = = ="
        );
    }
}
