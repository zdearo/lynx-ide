//! Adds Lynx's MCP server to an agent's own configuration, so the browser tools are there the next
//! time that agent starts. It goes in and out together with the agent's hooks: turning the agent
//! on in the settings is what asks for it. Only the `lynx` entry is ever touched.
//!
//! Claude Code keeps its servers in a file it rewrites constantly while it runs, so Lynx asks the
//! `claude` CLI to make the change. Codex and OpenCode keep theirs in plain config files, edited
//! here without disturbing the rest of the file.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde_json::{Map, Value, json};
use toml_edit::{Array, DocumentMut, Item, Table, value};

use crate::bounded_command::{BoundedError, run_bounded};

use super::harness::{HarnessId, harness_name};
use super::hook_hub::{PORT_ENV, SESSION_ENV, TOKEN_ENV};
use super::hook_install::{
    HookInstallError, invalid_config, io_error, read_optional, write_atomic,
};
use super::mcp_server::FLAG;

/// The name the server has in every agent's list.
pub const SERVER_NAME: &str = "lynx";
const CLI_TIMEOUT: Duration = Duration::from_secs(20);
/// Codex gives a tool 60 s by default; waiting for a page can take longer than that.
const CODEX_TOOL_TIMEOUT_SECS: i64 = 120;

/// Where each supported agent keeps its MCP servers.
#[derive(Clone)]
pub struct McpTargets {
    claude_config: PathBuf,
    codex_config: PathBuf,
    opencode_config: PathBuf,
    executable: Option<PathBuf>,
}

impl McpTargets {
    pub fn new(
        claude_config: PathBuf,
        codex_config: PathBuf,
        opencode_config: PathBuf,
        executable: Option<&Path>,
    ) -> Self {
        Self {
            claude_config,
            codex_config,
            opencode_config,
            executable: executable.map(Path::to_path_buf),
        }
    }

    /// Whether Lynx knows how to add its server to this agent.
    pub fn supports(harness: HarnessId) -> bool {
        matches!(
            harness,
            HarnessId::Claude | HarnessId::Codex | HarnessId::Opencode
        )
    }

    /// Adds or refreshes the server entry. An agent without MCP support here is left alone.
    pub fn install(&self, harness: HarnessId) -> Result<(), HookInstallError> {
        let executable = self.executable()?;
        match harness {
            HarnessId::Claude => self.install_claude(executable),
            HarnessId::Codex => self.install_codex(executable),
            HarnessId::Opencode => self.install_opencode(executable),
            _ => Ok(()),
        }
    }

    /// Adds the entry when the agent has none, or has one from before entries carried the agent's
    /// name, so a start-up pass never rewrites any other entry.
    pub fn install_if_missing(&self, harness: HarnessId) -> Result<(), HookInstallError> {
        if self.is_present(harness)? && !self.lacks_agent_name(harness)? {
            return Ok(());
        }
        self.install(harness)
    }

    /// Whether the entry ends at the flag, as the first entries did. Without the name the app
    /// cannot say which agent is driving the browser.
    fn lacks_agent_name(&self, harness: HarnessId) -> Result<bool, HookInstallError> {
        let last = match harness {
            HarnessId::Claude => claude_entry(&self.claude_config)?.and_then(|entry| {
                entry["args"]
                    .as_array()?
                    .last()?
                    .as_str()
                    .map(str::to_owned)
            }),
            HarnessId::Codex => codex_document(&self.codex_config)?
                .get("mcp_servers")
                .and_then(|servers| servers.get(SERVER_NAME))
                .and_then(|entry| entry.get("args")?.as_array()?.iter().last()?.as_str())
                .map(str::to_owned),
            HarnessId::Opencode => opencode_root(&self.opencode_config)?
                .get("mcp")
                .and_then(|servers| servers.get(SERVER_NAME))
                .and_then(|entry| {
                    entry["command"]
                        .as_array()?
                        .last()?
                        .as_str()
                        .map(str::to_owned)
                }),
            _ => None,
        };
        Ok(last.as_deref() == Some(FLAG))
    }

    pub fn remove(&self, harness: HarnessId) -> Result<(), HookInstallError> {
        match harness {
            HarnessId::Claude => self.remove_claude(),
            HarnessId::Codex => self.remove_codex(),
            HarnessId::Opencode => self.remove_opencode(),
            _ => Ok(()),
        }
    }

    /// Whether the agent has a `lynx` server, whichever program it points at.
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
            _ => true,
        })
    }

    fn executable(&self) -> Result<&Path, HookInstallError> {
        self.executable
            .as_deref()
            .ok_or(HookInstallError::NoExecutable)
    }

    fn install_claude(&self, executable: &Path) -> Result<(), HookInstallError> {
        let current = claude_entry(&self.claude_config)?;
        if current
            .as_ref()
            .is_some_and(|entry| claude_points_at(entry, executable))
        {
            return Ok(());
        }
        if current.is_some() {
            claude_cli(&["mcp", "remove", "--scope", "user", SERVER_NAME])?;
        }
        let program = executable.to_string_lossy();
        claude_cli(&[
            "mcp",
            "add",
            "--scope",
            "user",
            SERVER_NAME,
            "--",
            &program,
            FLAG,
            harness_name(HarnessId::Claude),
        ])
    }

    fn remove_claude(&self) -> Result<(), HookInstallError> {
        if claude_entry(&self.claude_config)?.is_none() {
            return Ok(());
        }
        claude_cli(&["mcp", "remove", "--scope", "user", SERVER_NAME])
    }

    fn install_codex(&self, executable: &Path) -> Result<(), HookInstallError> {
        let path = &self.codex_config;
        let mut document = codex_document(path)?;
        let servers = document
            .entry("mcp_servers")
            .or_insert_with(implicit_table)
            .as_table_mut()
            .ok_or_else(|| invalid_config(path))?;

        let mut entry = Table::new();
        entry["command"] = value(executable.to_string_lossy().into_owned());
        entry["args"] = value(Array::from_iter([FLAG, harness_name(HarnessId::Codex)]));
        // Codex starts a server with a trimmed environment; the session has to be named to reach it.
        entry["env_vars"] = value(Array::from_iter([SESSION_ENV, PORT_ENV, TOKEN_ENV]));
        entry["tool_timeout_sec"] = value(CODEX_TOOL_TIMEOUT_SECS);
        servers.insert(SERVER_NAME, Item::Table(entry));
        write_atomic(path, &document.to_string())
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

    fn install_opencode(&self, executable: &Path) -> Result<(), HookInstallError> {
        let path = &self.opencode_config;
        let mut root = opencode_root(path)?;
        if !path.exists() {
            root.insert(
                "$schema".to_owned(),
                json!("https://opencode.ai/config.json"),
            );
        }
        let servers = root
            .entry("mcp")
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .ok_or_else(|| invalid_config(path))?;
        servers.insert(
            SERVER_NAME.to_owned(),
            json!({
                "type": "local",
                "command": [executable.to_string_lossy(), FLAG, harness_name(HarnessId::Opencode)],
                "enabled": true,
            }),
        );
        write_json(path, &root)
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

fn implicit_table() -> Item {
    let mut table = Table::new();
    table.set_implicit(true);
    Item::Table(table)
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

fn claude_points_at(entry: &Value, executable: &Path) -> bool {
    entry.get("command").and_then(Value::as_str) == Some(&executable.to_string_lossy())
        && entry.get("args") == Some(&json!([FLAG, harness_name(HarnessId::Claude)]))
}

/// Runs `claude` with these arguments. A change the CLI refuses is reported with what it said.
fn claude_cli(arguments: &[&str]) -> Result<(), HookInstallError> {
    let mut command = Command::new("claude");
    command.args(arguments);
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
    use std::fs;

    use super::*;

    const EXE: &str = "/opt/lynx/lynx-ide";

    fn targets(dir: &Path) -> McpTargets {
        McpTargets::new(
            dir.join("claude.json"),
            dir.join("codex").join("config.toml"),
            dir.join("opencode").join("opencode.json"),
            Some(Path::new(EXE)),
        )
    }

    #[test]
    fn codex_gets_a_server_entry_that_keeps_the_rest_of_the_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let targets = targets(dir.path());
        let path = dir.path().join("codex").join("config.toml");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            "# my settings\nmodel = \"o3\"\n\n[mcp_servers.other]\ncommand = \"other\"\n",
        )
        .unwrap();

        targets.install(HarnessId::Codex).expect("install");

        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("# my settings"));
        assert!(text.contains("[mcp_servers.other]"));
        let document: DocumentMut = text.parse().unwrap();
        let ours = &document["mcp_servers"]["lynx"];
        assert_eq!(ours["command"].as_str(), Some(EXE));
        assert_eq!(
            ours["args"]
                .as_array()
                .unwrap()
                .get(0)
                .and_then(|v| v.as_str()),
            Some("--mcp")
        );
        assert_eq!(ours["env_vars"].as_array().unwrap().len(), 3);
        assert_eq!(ours["tool_timeout_sec"].as_integer(), Some(120));
    }

    #[test]
    fn codex_removal_takes_only_our_entry_and_tidies_an_empty_list() {
        let dir = tempfile::tempdir().expect("temp dir");
        let targets = targets(dir.path());
        let path = dir.path().join("codex").join("config.toml");
        targets.install(HarnessId::Codex).expect("install");

        targets.remove(HarnessId::Codex).expect("remove");

        let text = fs::read_to_string(&path).unwrap();
        assert!(!text.contains("lynx"));
        assert!(!text.contains("mcp_servers"));
        assert!(!targets.is_present(HarnessId::Codex).unwrap());
    }

    #[test]
    fn a_codex_file_that_is_not_toml_is_reported_and_never_overwritten() {
        let dir = tempfile::tempdir().expect("temp dir");
        let targets = targets(dir.path());
        let path = dir.path().join("codex").join("config.toml");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "model = [unterminated").unwrap();

        let error = targets.install(HarnessId::Codex).unwrap_err();

        assert!(matches!(error, HookInstallError::InvalidConfig { .. }));
        assert_eq!(fs::read_to_string(&path).unwrap(), "model = [unterminated");
    }

    #[test]
    fn opencode_gets_a_local_server_and_keeps_its_other_settings() {
        let dir = tempfile::tempdir().expect("temp dir");
        let targets = targets(dir.path());
        let path = dir.path().join("opencode").join("opencode.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            r#"{ "autoupdate": false, "mcp": { "other": { "type": "remote", "url": "https://x" } } }"#,
        )
        .unwrap();

        targets.install(HarnessId::Opencode).expect("install");

        let root: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(root["autoupdate"], false);
        assert_eq!(root["mcp"]["other"]["url"], "https://x");
        assert_eq!(
            root["mcp"]["lynx"],
            json!({ "type": "local", "command": [EXE, "--mcp", "opencode"], "enabled": true })
        );
    }

    #[test]
    fn opencode_removal_takes_only_our_entry() {
        let dir = tempfile::tempdir().expect("temp dir");
        let targets = targets(dir.path());
        let path = dir.path().join("opencode").join("opencode.json");
        targets.install(HarnessId::Opencode).expect("install");
        assert!(targets.is_present(HarnessId::Opencode).unwrap());

        targets.remove(HarnessId::Opencode).expect("remove");

        let root: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert!(root.get("mcp").is_none());
        assert!(!targets.is_present(HarnessId::Opencode).unwrap());
    }

    #[test]
    fn an_opencode_file_that_is_not_json_is_reported_and_never_overwritten() {
        let dir = tempfile::tempdir().expect("temp dir");
        let targets = targets(dir.path());
        let path = dir.path().join("opencode").join("opencode.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{ // a comment\n}").unwrap();

        let error = targets.install(HarnessId::Opencode).unwrap_err();

        assert!(matches!(error, HookInstallError::InvalidConfig { .. }));
        assert_eq!(fs::read_to_string(&path).unwrap(), "{ // a comment\n}");
    }

    #[test]
    fn an_install_pass_that_only_fills_gaps_leaves_an_existing_entry_alone() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("opencode").join("opencode.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let theirs =
            r#"{"mcp":{"lynx":{"type":"local","command":["/elsewhere/lynx","--mcp","opencode"]}}}"#;
        fs::write(&path, theirs).unwrap();

        targets(dir.path())
            .install_if_missing(HarnessId::Opencode)
            .expect("pass");

        assert_eq!(fs::read_to_string(&path).unwrap(), theirs);
    }

    #[test]
    fn claude_is_read_from_its_own_file_and_compared_with_this_executable() {
        let dir = tempfile::tempdir().expect("temp dir");
        let targets = targets(dir.path());
        fs::write(
            dir.path().join("claude.json"),
            json!({ "mcpServers": { "lynx": { "type": "stdio", "command": EXE, "args": ["--mcp", "claude"] } } })
                .to_string(),
        )
        .unwrap();

        assert!(targets.is_present(HarnessId::Claude).unwrap());
        let entry = claude_entry(&dir.path().join("claude.json"))
            .unwrap()
            .unwrap();
        assert!(claude_points_at(&entry, Path::new(EXE)));
        assert!(!claude_points_at(&entry, Path::new("/moved/lynx-ide")));
    }

    #[test]
    fn an_agent_without_mcp_support_here_is_left_alone() {
        let dir = tempfile::tempdir().expect("temp dir");
        let targets = targets(dir.path());

        targets.install(HarnessId::Cursor).expect("nothing to do");
        targets.remove(HarnessId::Cursor).expect("nothing to do");

        assert!(!McpTargets::supports(HarnessId::Cursor));
        assert!(McpTargets::supports(HarnessId::Claude));
    }

    #[test]
    fn without_the_app_executable_nothing_can_be_installed() {
        let dir = tempfile::tempdir().expect("temp dir");
        let targets = McpTargets::new(
            dir.path().join("a"),
            dir.path().join("b"),
            dir.path().join("c"),
            None,
        );
        assert!(matches!(
            targets.install(HarnessId::Codex),
            Err(HookInstallError::NoExecutable)
        ));
    }

    #[test]
    fn an_entry_from_before_the_agent_name_is_rewritten_with_it() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("opencode").join("opencode.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            r#"{"mcp":{"lynx":{"type":"local","command":["/old/lynx","--mcp"]}}}"#,
        )
        .unwrap();

        targets(dir.path())
            .install_if_missing(HarnessId::Opencode)
            .expect("pass");

        let root: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            root["mcp"]["lynx"]["command"],
            json!([EXE, "--mcp", "opencode"])
        );
    }
}
