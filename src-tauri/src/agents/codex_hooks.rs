//! Codex hooks: a handler in `hooks.json` that runs Lynx's relay for each event, and a trust
//! entry in `config.toml` for it. Codex runs a hook only once it is trusted, and the trust is a
//! hash of the hook's definition, which Lynx computes the way Codex does (checked against Codex
//! 0.160: the hashes of the hooks already trusted there match).

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use toml_edit::{DocumentMut, Item, Table, value};

use super::activity::HookEvent;
use super::hook_install::{
    HookInstallError, HookInstallState, HookInstallStatus, Version, invalid_config, io_error,
    read_optional, write_atomic,
};

/// Appended to the app's executable path; a hook command ending with it is Lynx's.
const COMMAND_SUFFIX: &str = " --agent-hook codex";

/// Codex versions before this one have no hook approvals, so their hooks need no trust entry.
const TRUST_FROM: Version = [0, 129, 0];

/// Every Lynx handler asks for this timeout. Codex defaults differ by event (SessionEnd gets one
/// second) and Interrupt is capped at three, so an explicit value keeps one hash rule for all.
/// The relay answers in milliseconds, so three seconds is never the limit.
const HOOK_TIMEOUT_SECS: u64 = 3;
/// What Codex assumes for a handler without a timeout, outside the events checked below.
const DEFAULT_TIMEOUT_SECS: u64 = 600;

/// A hook Lynx listens to: its name in `hooks.json` and its label in the trust keys.
/// Lynx writes no matchers, so every tool and every session source matches.
struct HookedEvent {
    name: &'static str,
    label: &'static str,
}

const HOOKED_EVENTS: &[HookedEvent] = &[
    HookedEvent {
        name: "SessionStart",
        label: "session_start",
    },
    HookedEvent {
        name: "SessionEnd",
        label: "session_end",
    },
    HookedEvent {
        name: "UserPromptSubmit",
        label: "user_prompt_submit",
    },
    HookedEvent {
        name: "PreToolUse",
        label: "pre_tool_use",
    },
    HookedEvent {
        name: "PostToolUse",
        label: "post_tool_use",
    },
    HookedEvent {
        name: "PermissionRequest",
        label: "permission_request",
    },
    HookedEvent {
        name: "Stop",
        label: "stop",
    },
    HookedEvent {
        name: "Interrupt",
        label: "interrupt",
    },
];

/// The event a Codex hook report stands for, or None when Lynx does not track it.
pub fn event_from_payload(payload: &Value) -> Option<HookEvent> {
    match payload.get("hook_event_name")?.as_str()? {
        "SessionStart" => Some(HookEvent::Started),
        "UserPromptSubmit" | "PreToolUse" | "PostToolUse" => Some(HookEvent::Working),
        "PermissionRequest" => Some(HookEvent::NeedsYou),
        // Interrupt is the only hook a turn cancelled with Esc fires, so it ends the turn too.
        "Stop" | "Interrupt" => Some(HookEvent::Finished),
        "SessionEnd" => Some(HookEvent::Ended),
        _ => None,
    }
}

/// The hash Codex trusts a handler by: SHA-256 of its definition, with the defaults filled in.
fn trusted_hash(event: &HookedEvent, handler: &Value) -> String {
    let timeout = normalized_timeout(event, handler);
    let command = handler
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or_default();
    // Keys are in sorted order, which is how Codex serialises the definition it hashes.
    let identity = json!({
        "event_name": event.label,
        "hooks": [{
            "async": false,
            "command": command,
            "timeout": timeout,
            "type": "command",
        }],
    });
    let serialized = serde_json::to_string(&identity).unwrap_or_default();
    let digest = Sha256::digest(serialized.as_bytes());
    let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("sha256:{hex}")
}

/// The timeout Codex hashes a handler with: the one written, or the default Codex uses for it.
fn normalized_timeout(event: &HookedEvent, handler: &Value) -> u64 {
    let given = handler.get("timeout").and_then(Value::as_u64);
    match event.label {
        "interrupt" => given.unwrap_or(1).clamp(1, HOOK_TIMEOUT_SECS),
        "session_end" => given.unwrap_or(1).max(1),
        _ => given.unwrap_or(DEFAULT_TIMEOUT_SECS).max(1),
    }
}

/// A handler's old (group, handler) position and its new one, or None when it is removed.
type Move = ((usize, usize), Option<(usize, usize)>);

/// The key Codex stores a handler's trust under: `<hooks.json path>:<label>:<group>:<handler>`.
fn trust_key(source: &str, label: &str, group: usize, handler: usize) -> String {
    format!("{source}:{label}:{group}:{handler}")
}

/// Splits a trust key back into its label, group and handler, if it belongs to `source`. The
/// source is the rest of the key from the left, since a Windows path has colons of its own.
fn parse_trust_key(key: &str, source: &str) -> Option<(String, usize, usize)> {
    let mut parts = key.rsplitn(4, ':');
    let handler = parts.next()?.parse().ok()?;
    let group = parts.next()?.parse().ok()?;
    let label = parts.next()?;
    let found_source = parts.next()?;
    (found_source == source).then(|| (label.to_owned(), group, handler))
}

fn is_lynx_handler(handler: &Value) -> bool {
    handler
        .get("command")
        .and_then(Value::as_str)
        .is_some_and(|command| command.ends_with(COMMAND_SUFFIX))
}

fn handlers_of(group: &Value) -> &[Value] {
    match group.get("hooks").and_then(Value::as_array) {
        Some(handlers) => handlers.as_slice(),
        None => &[],
    }
}

/// The handler list of an event, if the file has one.
fn groups_of<'a>(hooks: &'a Value, event: &str) -> &'a [Value] {
    let groups = hooks
        .get("hooks")
        .and_then(|events| events.get(event))
        .and_then(Value::as_array);
    match groups {
        Some(groups) => groups.as_slice(),
        None => &[],
    }
}

fn command_handler(command: &str) -> Value {
    json!({ "type": "command", "command": command, "timeout": HOOK_TIMEOUT_SECS })
}

/// The trust entry Codex keeps for a handler, if there is one.
fn trust_value<'a>(config: &'a DocumentMut, key: &str) -> Option<&'a str> {
    config
        .get("hooks")?
        .get("state")?
        .get(key)?
        .get("trusted_hash")?
        .as_str()
}

fn hooks_enabled(config: &DocumentMut) -> bool {
    config
        .get("features")
        .and_then(|features| features.get("hooks"))
        .and_then(Item::as_bool)
        == Some(true)
}

fn read_hooks(path: &Path) -> Result<Value, HookInstallError> {
    let Some(text) = read_optional(path)? else {
        return Ok(json!({ "hooks": {} }));
    };
    if text.trim().is_empty() {
        return Ok(json!({ "hooks": {} }));
    }
    let hooks: Value = serde_json::from_str(&text).map_err(|_| invalid_config(path))?;
    let valid = hooks.as_object().is_some_and(|root| {
        root.get("hooks")
            .is_none_or(|events| events.as_object().is_some())
    });
    if valid {
        Ok(hooks)
    } else {
        Err(invalid_config(path))
    }
}

fn read_config(path: &Path) -> Result<DocumentMut, HookInstallError> {
    let Some(text) = read_optional(path)? else {
        return Ok(DocumentMut::new());
    };
    text.parse().map_err(|_| invalid_config(path))
}

/// Takes Lynx's handlers out of `hooks.json`. The trust entries of the handlers that move to
/// fill the gap are renamed to match, so each trust entry keeps the hook it was given to.
/// Returns whether anything was removed.
fn strip(hooks: &mut Value, config: &mut DocumentMut, source: &str) -> bool {
    let mut changed = false;
    for event in HOOKED_EVENTS {
        let groups = groups_of(hooks, event.name);
        if !groups
            .iter()
            .any(|group| handlers_of(group).iter().any(is_lynx_handler))
        {
            continue;
        }
        // Each old (group, handler) position maps to its new one, or to nothing when removed.
        let mut moves: Vec<Move> = Vec::new();
        let mut kept_groups = Vec::new();
        for (group_index, group) in groups.iter().enumerate() {
            let handlers = handlers_of(group);
            let mut kept = Vec::new();
            for (handler_index, handler) in handlers.iter().enumerate() {
                if is_lynx_handler(handler) {
                    moves.push(((group_index, handler_index), None));
                } else {
                    moves.push((
                        (group_index, handler_index),
                        Some((kept_groups.len(), kept.len())),
                    ));
                    kept.push(handler.clone());
                }
            }
            if kept.is_empty() && !handlers.is_empty() {
                continue;
            }
            let mut group = group.clone();
            if let Some(object) = group.as_object_mut() {
                object.insert("hooks".to_owned(), Value::Array(kept));
            }
            kept_groups.push(group);
        }
        if let Some(events) = hooks.get_mut("hooks").and_then(Value::as_object_mut) {
            events.insert(event.name.to_owned(), Value::Array(kept_groups));
        }
        rekey_trust(config, source, event.label, &moves);
        changed = true;
    }
    changed
}

/// Moves or drops the trust entries of one event after its handlers were renumbered. All the
/// old keys come out before any new one goes in, so a move cannot overwrite an entry not yet read.
fn rekey_trust(config: &mut DocumentMut, source: &str, label: &str, moves: &[Move]) {
    let Some(state) = config
        .get_mut("hooks")
        .and_then(|hooks| hooks.get_mut("state"))
        .and_then(Item::as_table_mut)
    else {
        return;
    };
    // (old key, new key); a new key of None means the entry goes.
    let mut renames: Vec<(String, Option<String>)> = Vec::new();
    for (key, _) in state.iter() {
        let Some((found_label, group, handler)) = parse_trust_key(key, source) else {
            continue;
        };
        if found_label != label {
            continue;
        }
        match moves.iter().find(|(from, _)| *from == (group, handler)) {
            Some((_, Some((new_group, new_handler)))) => {
                let new_key = trust_key(source, label, *new_group, *new_handler);
                if new_key != key {
                    renames.push((key.to_owned(), Some(new_key)));
                }
            }
            Some((_, None)) => renames.push((key.to_owned(), None)),
            None => {}
        }
    }
    let mut carried = Vec::new();
    for (old, new) in renames {
        let item = state.remove(&old);
        if let (Some(item), Some(new)) = (item, new) {
            carried.push((new, item));
        }
    }
    for (new, item) in carried {
        state.insert(&new, item);
    }
}

/// Sets a handler's trust, creating the tables that hold it when they are missing.
fn set_trust(config: &mut DocumentMut, key: &str, hash: &str) {
    let Some(hooks) = config
        .entry("hooks")
        .or_insert(implicit_table())
        .as_table_like_mut()
    else {
        return;
    };
    let Some(state) = hooks
        .entry("state")
        .or_insert(implicit_table())
        .as_table_like_mut()
    else {
        return;
    };
    if let Some(entry) = state
        .entry(key)
        .or_insert(Item::Table(Table::new()))
        .as_table_mut()
    {
        entry.insert("trusted_hash", value(hash));
    }
}

fn implicit_table() -> Item {
    let mut table = Table::new();
    table.set_implicit(true);
    Item::Table(table)
}

#[derive(Clone)]
pub struct CodexHookTarget {
    /// The folder Codex keeps its settings in: `CODEX_HOME`, or `~/.codex`.
    home: PathBuf,
    command: Option<String>,
}

impl CodexHookTarget {
    pub fn new(home: PathBuf, executable: Option<&Path>) -> Self {
        Self {
            home,
            command: executable.map(|path| {
                let path = path.to_string_lossy();
                let quoted = if cfg!(windows) {
                    format!("\"{path}\"")
                } else {
                    format!("'{}'", path.replace('\'', r"'\''"))
                };
                format!("{quoted}{COMMAND_SUFFIX}")
            }),
        }
    }

    fn hooks_path(&self) -> PathBuf {
        self.home.join("hooks.json")
    }

    fn config_path(&self) -> PathBuf {
        self.home.join("config.toml")
    }

    /// The key Codex files its trust under: the hooks file as Codex sees it.
    fn source(&self) -> String {
        self.hooks_path().to_string_lossy().into_owned()
    }

    pub fn status(&self, version: Option<Version>) -> HookInstallStatus {
        let read = read_hooks(&self.hooks_path())
            .and_then(|hooks| read_config(&self.config_path()).map(|config| (hooks, config)));
        match read {
            Ok((hooks, config)) => self.describe(&hooks, &config, version),
            Err(error) => self.status_with(HookInstallState::Error, Some(error.to_string())),
        }
    }

    /// Adds Lynx's handlers and trusts them. Other tools' hooks and their trust stay as they are.
    pub fn install(&self, version: Option<Version>) -> Result<HookInstallStatus, HookInstallError> {
        let command = self
            .command
            .as_deref()
            .ok_or(HookInstallError::NoExecutable)?;
        let source = self.source();
        let mut hooks = read_hooks(&self.hooks_path())?;
        let mut config = read_config(&self.config_path())?;

        strip(&mut hooks, &mut config, &source);

        let trusting = trusts(version);
        let invalid = || invalid_config(&self.hooks_path());
        let events = hooks
            .as_object_mut()
            .and_then(|root| {
                root.entry("hooks")
                    .or_insert_with(|| json!({}))
                    .as_object_mut()
            })
            .ok_or_else(invalid)?;
        let mut trust = Vec::new();
        for event in HOOKED_EVENTS {
            let groups = events
                .entry(event.name)
                .or_insert_with(|| json!([]))
                .as_array_mut()
                .ok_or_else(invalid)?;
            let handler = command_handler(command);
            groups.push(json!({ "hooks": [handler.clone()] }));
            if trusting {
                let key = trust_key(&source, event.label, groups.len() - 1, 0);
                trust.push((key, trusted_hash(event, &handler)));
            }
        }
        for (key, hash) in &trust {
            set_trust(&mut config, key, hash);
        }
        if config.get("features").is_none() {
            config.insert("features", Item::Table(Table::new()));
        }
        if let Some(features) = config.get_mut("features").and_then(Item::as_table_like_mut) {
            features.insert("hooks", value(true));
        }

        // The trust goes first, so a hook is never present without its approval.
        write_atomic(&self.config_path(), &config.to_string())?;
        let text = serde_json::to_string_pretty(&hooks)
            .map_err(|error| io_error(&self.hooks_path(), std::io::Error::other(error)))?;
        write_atomic(&self.hooks_path(), &format!("{text}\n"))?;
        Ok(self.status(version))
    }

    /// Removes Lynx's handlers and their trust, and leaves everything else as it was.
    pub fn remove(&self, version: Option<Version>) -> Result<HookInstallStatus, HookInstallError> {
        let source = self.source();
        let mut hooks = read_hooks(&self.hooks_path())?;
        let mut config = read_config(&self.config_path())?;
        if strip(&mut hooks, &mut config, &source) {
            write_atomic(&self.config_path(), &config.to_string())?;
            let text = serde_json::to_string_pretty(&hooks)
                .map_err(|error| io_error(&self.hooks_path(), std::io::Error::other(error)))?;
            write_atomic(&self.hooks_path(), &format!("{text}\n"))?;
        }
        Ok(self.status(version))
    }

    fn describe(
        &self,
        hooks: &Value,
        config: &DocumentMut,
        version: Option<Version>,
    ) -> HookInstallStatus {
        let Some(command) = self.command.as_deref() else {
            return self.status_with(
                HookInstallState::Error,
                Some(HookInstallError::NoExecutable.to_string()),
            );
        };
        let source = self.source();
        let trusting = trusts(version);
        let mut missing = Vec::new();
        for event in HOOKED_EVENTS {
            let groups = groups_of(hooks, event.name);
            let ours = groups.iter().enumerate().find_map(|(group_index, group)| {
                handlers_of(group)
                    .iter()
                    .enumerate()
                    .find(|(_, handler)| {
                        handler.get("command").and_then(Value::as_str) == Some(command)
                    })
                    .map(|(handler_index, handler)| (group_index, handler_index, handler))
            });
            let healthy = match ours {
                None => false,
                Some((group_index, handler_index, handler)) => {
                    !trusting
                        || trust_value(
                            config,
                            &trust_key(&source, event.label, group_index, handler_index),
                        ) == Some(trusted_hash(event, handler).as_str())
                }
            };
            if !healthy {
                missing.push(event.name);
            }
        }
        let enabled = hooks_enabled(config);

        let state = if missing.len() == HOOKED_EVENTS.len() {
            HookInstallState::NotInstalled
        } else if missing.is_empty() && enabled {
            HookInstallState::Installed
        } else {
            HookInstallState::Partial
        };
        let mut notes = Vec::new();
        if version.is_none() {
            notes.push("Could not read the Codex version, so the hooks are trusted as the newest Codex expects.".to_owned());
        }
        if !missing.is_empty() {
            notes.push(format!("Missing hooks for: {}", missing.join(", ")));
        }
        if !enabled && state != HookInstallState::NotInstalled {
            notes.push("Codex hooks are turned off in config.toml.".to_owned());
        }
        let detail = (!notes.is_empty()).then(|| notes.join(" "));
        self.status_with(state, detail)
    }

    fn status_with(&self, state: HookInstallState, detail: Option<String>) -> HookInstallStatus {
        HookInstallStatus {
            state,
            config_path: self.config_path().to_string_lossy().into_owned(),
            events: HOOKED_EVENTS
                .iter()
                .map(|event| event.name.to_owned())
                .collect(),
            detail,
        }
    }
}

/// Whether Codex asks for approval of hooks at this version, so a trust entry is needed.
fn trusts(version: Option<Version>) -> bool {
    version.is_none_or(|version| version >= TRUST_FROM)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    const COMMAND: &str = "'/opt/lynx/lynx-ide' --agent-hook codex";
    /// The command the vectors below were checked against, with Codex 0.160.1. The hash depends on
    /// the command, so it keeps the name the app had then; the vectors are not recomputed.
    const VERIFIED_COMMAND: &str = "'/opt/lynx/agent-ade' --agent-hook codex";

    fn event(name: &str) -> &'static HookedEvent {
        HOOKED_EVENTS
            .iter()
            .find(|event| event.name == name)
            .expect("known event")
    }

    fn handler() -> Value {
        command_handler(COMMAND)
    }

    /// The vectors were computed from the definition Codex hashes, outside this code, so these
    /// tests check the algorithm and not just that the code agrees with itself.
    #[test]
    fn trusted_hashes_match_the_definition_codex_hashes() {
        assert_eq!(
            trusted_hash(event("Stop"), &command_handler(VERIFIED_COMMAND)),
            "sha256:9303631a9bb6b4928371fd9c5e04b7f300d54a60eeff3fdb9309ad2ec2ec756d"
        );
        assert_eq!(
            trusted_hash(event("Interrupt"), &command_handler(VERIFIED_COMMAND)),
            "sha256:dffbbb154cedc9a4b67bffa2429e84ee43ef1498b3efaad93617b0fc36ce11ab"
        );
    }

    #[test]
    fn every_lynx_handler_asks_for_the_same_timeout() {
        for event in HOOKED_EVENTS {
            let handler = handler();
            assert_eq!(handler["timeout"], HOOK_TIMEOUT_SECS, "{}", event.name);
        }
    }

    #[test]
    fn interrupt_is_capped_at_three_seconds_and_session_end_defaults_to_one() {
        let long = json!({ "command": "x", "timeout": 60 });
        assert_eq!(normalized_timeout(event("Interrupt"), &long), 3);
        assert_eq!(
            normalized_timeout(event("SessionEnd"), &json!({ "command": "x" })),
            1
        );
    }

    #[test]
    fn a_trust_key_is_read_back_for_its_own_file_only() {
        let source = "/home/me/.codex/hooks.json";
        let key = trust_key(source, "stop", 2, 0);
        assert_eq!(
            parse_trust_key(&key, source),
            Some(("stop".to_owned(), 2, 0))
        );
        assert_eq!(parse_trust_key(&key, "/elsewhere/hooks.json"), None);
    }

    #[test]
    fn the_hook_events_map_to_what_the_activity_tracks() {
        let event = |name: &str| json!({ "hook_event_name": name });
        assert_eq!(
            event_from_payload(&event("SessionStart")),
            Some(HookEvent::Started)
        );
        assert_eq!(
            event_from_payload(&event("PreToolUse")),
            Some(HookEvent::Working)
        );
        assert_eq!(
            event_from_payload(&event("PermissionRequest")),
            Some(HookEvent::NeedsYou)
        );
        assert_eq!(
            event_from_payload(&event("Interrupt")),
            Some(HookEvent::Finished)
        );
        assert_eq!(
            event_from_payload(&event("SessionEnd")),
            Some(HookEvent::Ended)
        );
        assert_eq!(event_from_payload(&event("SubagentStop")), None);
    }

    #[test]
    fn install_writes_our_handlers_and_their_trust_and_turns_hooks_on() {
        let dir = tempfile::tempdir().expect("temp dir");
        let target = CodexHookTarget::new(
            dir.path().to_path_buf(),
            Some(Path::new("/opt/lynx/lynx-ide")),
        );

        let status = target.install(Some([0, 160, 1])).expect("install");

        assert_eq!(status.state, HookInstallState::Installed);
        let config = fs::read_to_string(dir.path().join("config.toml")).expect("config");
        assert!(config.contains("hooks = true"));
        let source = dir.path().join("hooks.json").to_string_lossy().into_owned();
        // Read back as TOML: the quotes toml_edit picks for a Windows path do not matter.
        let document: DocumentMut = config.parse().expect("config.toml");
        assert!(
            trust_value(&document, &format!("{source}:stop:0:0")).is_some(),
            "{config}"
        );
    }

    #[test]
    fn install_keeps_other_tools_hooks_and_their_trust_and_comments() {
        let dir = tempfile::tempdir().expect("temp dir");
        let source = dir.path().join("hooks.json").to_string_lossy().into_owned();
        fs::write(
            dir.path().join("hooks.json"),
            json!({ "hooks": { "Stop": [{ "hooks": [{ "type": "command", "command": "paseo hooks codex Stop" }] }] } }).to_string(),
        )
        .expect("write hooks");
        fs::write(
            dir.path().join("config.toml"),
            format!(
                "# my codex settings\nmodel = \"x\"\n\n[hooks.state.'{source}:stop:0:0']\ntrusted_hash = \"sha256:other\"\n"
            ),
        )
        .expect("write config");
        let target = CodexHookTarget::new(
            dir.path().to_path_buf(),
            Some(Path::new("/opt/lynx/lynx-ide")),
        );

        target.install(Some([0, 160, 1])).expect("install");

        let config = fs::read_to_string(dir.path().join("config.toml")).expect("config");
        assert!(config.contains("# my codex settings"), "{config}");
        assert!(
            config.contains("trusted_hash = \"sha256:other\""),
            "{config}"
        );
        let hooks: Value = serde_json::from_str(
            &fs::read_to_string(dir.path().join("hooks.json")).expect("hooks"),
        )
        .expect("json");
        let stop = hooks["hooks"]["Stop"].as_array().expect("stop groups");
        assert_eq!(stop.len(), 2);
        assert_eq!(stop[0]["hooks"][0]["command"], "paseo hooks codex Stop");
    }

    #[test]
    fn remove_moves_later_trust_entries_back_with_their_hook() {
        let dir = tempfile::tempdir().expect("temp dir");
        let source = dir.path().join("hooks.json").to_string_lossy().into_owned();
        let other = json!({ "hooks": [{ "type": "command", "command": "other tool" }] });
        let ours = json!({ "hooks": [{ "type": "command", "command": COMMAND }] });
        fs::write(
            dir.path().join("hooks.json"),
            json!({ "hooks": { "Stop": [ours, other] } }).to_string(),
        )
        .expect("write hooks");
        fs::write(
            dir.path().join("config.toml"),
            format!(
                "[hooks.state.'{source}:stop:0:0']\ntrusted_hash = \"sha256:ours\"\n\n[hooks.state.'{source}:stop:1:0']\ntrusted_hash = \"sha256:other\"\n"
            ),
        )
        .expect("write config");
        let target = CodexHookTarget::new(
            dir.path().to_path_buf(),
            Some(Path::new("/opt/lynx/lynx-ide")),
        );

        target.remove(Some([0, 160, 1])).expect("remove");

        let config = fs::read_to_string(dir.path().join("config.toml")).expect("config");
        let document: DocumentMut = config.parse().expect("config.toml");
        assert_eq!(
            trust_value(&document, &format!("{source}:stop:0:0")),
            Some("sha256:other"),
            "{config}"
        );
        assert!(config.contains("sha256:other"), "{config}");
        assert!(!config.contains("sha256:ours"), "{config}");
        assert!(!config.contains(":stop:1:0"), "{config}");
    }

    #[test]
    fn an_older_codex_needs_no_trust_entry() {
        let dir = tempfile::tempdir().expect("temp dir");
        let target = CodexHookTarget::new(
            dir.path().to_path_buf(),
            Some(Path::new("/opt/lynx/lynx-ide")),
        );

        let status = target.install(Some([0, 128, 0])).expect("install");

        assert_eq!(status.state, HookInstallState::Installed);
        let config = fs::read_to_string(dir.path().join("config.toml")).expect("config");
        assert!(!config.contains("trusted_hash"));
    }

    #[test]
    fn hooks_turned_off_in_codex_is_reported_as_partial() {
        let dir = tempfile::tempdir().expect("temp dir");
        let target = CodexHookTarget::new(
            dir.path().to_path_buf(),
            Some(Path::new("/opt/lynx/lynx-ide")),
        );
        target.install(Some([0, 160, 1])).expect("install");
        let config = fs::read_to_string(dir.path().join("config.toml")).expect("config");
        fs::write(
            dir.path().join("config.toml"),
            config.replace("hooks = true", "hooks = false"),
        )
        .expect("write");

        let status = target.status(Some([0, 160, 1]));

        assert_eq!(status.state, HookInstallState::Partial);
        assert!(status.detail.unwrap_or_default().contains("turned off"));
    }

    #[test]
    fn a_hooks_file_that_is_not_json_is_reported_and_never_overwritten() {
        let dir = tempfile::tempdir().expect("temp dir");
        fs::write(dir.path().join("hooks.json"), "{ nope").expect("write");
        let target = CodexHookTarget::new(
            dir.path().to_path_buf(),
            Some(Path::new("/opt/lynx/lynx-ide")),
        );

        assert_eq!(target.status(None).state, HookInstallState::Error);
        assert!(target.install(None).is_err());
        assert_eq!(
            fs::read_to_string(dir.path().join("hooks.json")).expect("read"),
            "{ nope"
        );
    }
}
