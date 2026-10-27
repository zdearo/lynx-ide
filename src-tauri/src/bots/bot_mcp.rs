//! The MCP servers a person gives one bot. They live in the bot's folder as `mcp.json`, in the format
//! Claude Code reads (`{"mcpServers": {...}}`), and every run hands them to the CLI. They may hold
//! secrets in their environment or headers, so the file is private, and the screen lists only each
//! server's name and where it runs.

use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use specta::Type;

use super::bot_bridge::SERVER_NAME;
use super::bot_folder::BotError;
use crate::private_file;

const FILE: &str = "mcp.json";
const MAX_SERVERS: usize = 16;
const MAX_TEXT: usize = 16 * 1024;
const MAX_NAME: usize = 40;

/// How a server is reached: a command it starts, or an address it is called at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum McpKind {
    Command,
    Url,
}

/// A server as the details tab lists it. Its environment and headers stay out of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BotMcpServer {
    pub name: String,
    pub kind: McpKind,
    /// The command the server starts, or the address it is called at.
    pub target: String,
}

/// The bot's servers as the CLI reads them, keyed by name, with their secrets.
pub fn servers(bot_dir: &Path) -> Result<Map<String, Value>, BotError> {
    read(bot_dir)
}

/// The bot's servers by name, for the details tab.
pub fn list(bot_dir: &Path) -> Result<Vec<BotMcpServer>, BotError> {
    let mut listed: Vec<BotMcpServer> = read(bot_dir)?
        .iter()
        .map(|(name, entry)| {
            let (kind, target) = entry
                .as_object()
                .and_then(transport)
                .unwrap_or((McpKind::Command, String::new()));
            BotMcpServer {
                name: name.clone(),
                kind,
                target,
            }
        })
        .collect();
    listed.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(listed)
}

/// Adds the servers in `text`, a `{"mcpServers": {...}}` config. A server with a name the bot has
/// already is replaced. Returns the bot's servers after the change.
pub fn add(bot_dir: &Path, text: &str) -> Result<Vec<BotMcpServer>, BotError> {
    if text.len() > MAX_TEXT {
        return Err(BotError::InvalidMcp(
            "the config is longer than 16 KB".to_owned(),
        ));
    }
    let config: Value = serde_json::from_str(text)
        .map_err(|_| BotError::InvalidMcp("it is not valid JSON".to_owned()))?;
    let incoming = config
        .get("mcpServers")
        .and_then(Value::as_object)
        .ok_or_else(|| BotError::InvalidMcp("it needs an \"mcpServers\" object".to_owned()))?;
    if incoming.is_empty() {
        return Err(BotError::InvalidMcp("it has no server in it".to_owned()));
    }
    let mut all = read(bot_dir)?;
    for (name, entry) in incoming {
        check(name, entry)?;
        all.insert(name.clone(), entry.clone());
    }
    if all.len() > MAX_SERVERS {
        return Err(BotError::InvalidMcp(format!(
            "a bot can have up to {MAX_SERVERS} servers"
        )));
    }
    write(bot_dir, &all)?;
    list(bot_dir)
}

/// Removes a server from the bot. Returns the bot's servers after the removal.
pub fn remove(bot_dir: &Path, name: &str) -> Result<Vec<BotMcpServer>, BotError> {
    let mut all = read(bot_dir)?;
    if all.remove(name).is_none() {
        return Err(BotError::UnknownMcp);
    }
    write(bot_dir, &all)?;
    list(bot_dir)
}

/// The servers in the file. A missing file has none; one that is not a config is refused, so a run
/// does not start without the servers the person meant it to have.
fn read(bot_dir: &Path) -> Result<Map<String, Value>, BotError> {
    let text = match fs::read_to_string(bot_dir.join(FILE)) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Map::new()),
        Err(error) => return Err(error.into()),
    };
    let config: Value = serde_json::from_str(&text)
        .map_err(|_| BotError::InvalidMcp("the bot's mcp.json is not valid JSON".to_owned()))?;
    Ok(config
        .get("mcpServers")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default())
}

fn write(bot_dir: &Path, servers: &Map<String, Value>) -> Result<(), BotError> {
    let text = serde_json::to_string_pretty(&json!({ "mcpServers": servers }))
        .map_err(std::io::Error::other)?;
    private_file::write(&bot_dir.join(FILE), text.as_bytes())?;
    Ok(())
}

/// A server's kind and where it is reached, from the fields it has. `type` defaults to `http` when
/// there is a `url`, and to `stdio` otherwise, as Claude Code reads it.
fn transport(entry: &Map<String, Value>) -> Option<(McpKind, String)> {
    let default = if entry.contains_key("url") {
        "http"
    } else {
        "stdio"
    };
    match entry.get("type").and_then(Value::as_str).unwrap_or(default) {
        "stdio" => Some((McpKind::Command, entry.get("command")?.as_str()?.to_owned())),
        "http" | "sse" => Some((McpKind::Url, entry.get("url")?.as_str()?.to_owned())),
        _ => None,
    }
}

/// Refuses a server Lynx cannot run: one that takes the name of Lynx's own server, a name that is
/// not lowercase letters, digits and dashes, or a shape Claude Code would not read.
fn check(name: &str, entry: &Value) -> Result<(), BotError> {
    let refused = |why: &str| BotError::InvalidMcp(format!("{name}: {why}"));
    if name == SERVER_NAME {
        return Err(refused("that name is Lynx's own server"));
    }
    if !valid_name(name) {
        return Err(refused(
            "the name needs lowercase letters, digits and dashes",
        ));
    }
    let object = entry
        .as_object()
        .ok_or_else(|| refused("it must be an object"))?;
    let declared = object.get("type").and_then(Value::as_str);
    if !matches!(declared, None | Some("stdio" | "http" | "sse")) {
        return Err(refused("its type is stdio, http or sse"));
    }
    match transport(object) {
        Some((McpKind::Command, command)) if command.trim().is_empty() => {
            return Err(refused("its command is empty"));
        }
        Some((McpKind::Url, url))
            if !(url.starts_with("https://") || url.starts_with("http://")) =>
        {
            return Err(refused("its url must start with http:// or https://"));
        }
        Some(_) => {}
        None => return Err(refused("it needs a command, or a url for http or sse")),
    }
    for key in ["args", "env", "headers"] {
        if !is_text(object.get(key)) {
            return Err(refused(&format!("{key} must hold text only")));
        }
    }
    Ok(())
}

/// Absent, or a list or an object of text only.
fn is_text(value: Option<&Value>) -> bool {
    match value {
        None => true,
        Some(Value::Array(items)) => items.iter().all(Value::is_string),
        Some(Value::Object(map)) => map.values().all(Value::is_string),
        Some(_) => false,
    }
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().count() <= MAX_NAME
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bot_folder() -> tempfile::TempDir {
        tempfile::tempdir().expect("temp dir")
    }

    const CONFIG: &str = r#"{"mcpServers": {
        "github": {"command": "npx", "args": ["-y", "@mcp/github"], "env": {"GITHUB_TOKEN": "segredo"}},
        "docs": {"type": "http", "url": "https://docs.example.com/mcp", "headers": {"Authorization": "Bearer segredo"}}
    }}"#;

    #[test]
    fn the_servers_are_listed_without_their_secrets_and_kept_for_the_run() {
        let dir = bot_folder();

        let listed = add(dir.path(), CONFIG).expect("added");

        assert_eq!(
            listed,
            vec![
                BotMcpServer {
                    name: "docs".to_owned(),
                    kind: McpKind::Url,
                    target: "https://docs.example.com/mcp".to_owned(),
                },
                BotMcpServer {
                    name: "github".to_owned(),
                    kind: McpKind::Command,
                    target: "npx".to_owned(),
                },
            ]
        );
        assert!(
            !serde_json::to_string(&listed)
                .expect("json")
                .contains("segredo")
        );
        let run = servers(dir.path()).expect("read");
        assert_eq!(run["github"]["env"]["GITHUB_TOKEN"], "segredo");
    }

    #[cfg(unix)]
    #[test]
    fn the_file_is_readable_only_by_its_owner() {
        use std::os::unix::fs::PermissionsExt;

        let dir = bot_folder();
        add(dir.path(), CONFIG).expect("added");

        let mode = fs::metadata(dir.path().join(FILE))
            .expect("file")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn a_name_that_is_lynxs_own_or_not_a_plain_name_is_refused() {
        let dir = bot_folder();
        for name in ["lynx-bot", "GitHub", "dois nomes", ""] {
            let config = format!(r#"{{"mcpServers": {{"{name}": {{"command": "x"}}}}}}"#);
            assert!(
                matches!(add(dir.path(), &config), Err(BotError::InvalidMcp(_))),
                "{name:?} is refused"
            );
        }
        assert!(list(dir.path()).expect("list").is_empty());
    }

    #[test]
    fn a_server_needs_a_command_or_an_http_address_and_text_arguments() {
        let dir = bot_folder();
        for entry in [
            r#"{}"#,
            r#"{"command": ""}"#,
            r#"{"type": "http", "url": "ftp://x"}"#,
            r#"{"type": "ws", "url": "https://x"}"#,
            r#"{"command": "npx", "args": "tudo numa string"}"#,
            r#"{"command": "npx", "env": {"N": 1}}"#,
        ] {
            let config = format!(r#"{{"mcpServers": {{"srv": {entry}}}}}"#);
            assert!(
                matches!(add(dir.path(), &config), Err(BotError::InvalidMcp(_))),
                "{entry} is refused"
            );
        }
    }

    #[test]
    fn text_that_is_not_a_config_is_refused_and_the_file_is_left_alone() {
        let dir = bot_folder();
        add(dir.path(), CONFIG).expect("added");

        for text in ["não é json", r#"{"outro": {}}"#, r#"{"mcpServers": {}}"#] {
            assert!(matches!(
                add(dir.path(), text),
                Err(BotError::InvalidMcp(_))
            ));
        }
        assert_eq!(list(dir.path()).expect("list").len(), 2);
    }

    #[test]
    fn a_server_with_the_same_name_is_replaced_and_too_many_are_refused() {
        let dir = bot_folder();
        add(dir.path(), CONFIG).expect("added");
        let replaced = add(
            dir.path(),
            r#"{"mcpServers": {"github": {"command": "outro"}}}"#,
        )
        .expect("replaced");
        assert_eq!(replaced.len(), 2);
        assert_eq!(replaced[1].target, "outro");

        let many: Vec<String> = (0..MAX_SERVERS)
            .map(|index| format!(r#""s{index}": {{"command": "x"}}"#))
            .collect();
        let config = format!(r#"{{"mcpServers": {{{}}}}}"#, many.join(","));
        assert!(matches!(
            add(dir.path(), &config),
            Err(BotError::InvalidMcp(_))
        ));
    }

    #[test]
    fn a_server_is_removed_by_name_and_an_unknown_one_is_refused() {
        let dir = bot_folder();
        add(dir.path(), CONFIG).expect("added");

        assert!(matches!(
            remove(dir.path(), "nao-existe"),
            Err(BotError::UnknownMcp)
        ));
        let left = remove(dir.path(), "docs").expect("removed");
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].name, "github");
    }
}
