//! The permission that lets an agent run the app's CLI without asking for every call. Claude has its own rule
//! in its settings (`claude_hooks`). Codex reads a rules file in its `rules` folder, and OpenCode reads a
//! `bash` permission per command pattern in its config. Lynx writes only its own entry, and removes only that.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use super::hook_install::{HookInstallError, invalid_config, read_optional, write_atomic};

/// The name of Lynx's Codex rules file, beside the rules the person keeps in the same folder.
const CODEX_RULES_FILE: &str = "lynx-cli.rules";
/// The end of the OpenCode pattern Lynx adds, which tells it apart from the person's own patterns.
const OPENCODE_RULE_END: &str = " --cli *";

/// The Codex rules file for the app's executable: the CLI is allowed, and nothing else is changed.
pub fn codex_rules(executable: &Path) -> String {
    let program = json!(executable.to_string_lossy()).to_string();
    format!(
        "# Written by Lynx: lets the agent run Lynx's CLI without asking.\nprefix_rule(\n    pattern = [{program}, \"--cli\"],\n    decision = \"allow\",\n)\n"
    )
}

/// Writes Lynx's Codex rule for `executable`, or removes it when there is none.
pub fn write_codex_rule(
    codex_home: &Path,
    executable: Option<&Path>,
) -> Result<(), HookInstallError> {
    let path = codex_rules_path(codex_home);
    match executable {
        Some(executable) => {
            std::fs::create_dir_all(codex_home.join("rules"))
                .map_err(|error| super::hook_install::io_error(&path, error))?;
            write_atomic(&path, &codex_rules(executable))
        }
        None => match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(super::hook_install::io_error(&path, error)),
        },
    }
}

fn codex_rules_path(codex_home: &Path) -> PathBuf {
    codex_home.join("rules").join(CODEX_RULES_FILE)
}

/// Sets Lynx's OpenCode `bash` permission in `config`, which is `opencode.json`. The person's own patterns
/// stay as they are, and a pattern from an earlier build is replaced. `None` removes Lynx's pattern.
pub fn write_opencode_rule(
    config: &Path,
    executable: Option<&Path>,
) -> Result<(), HookInstallError> {
    let Some(text) = read_optional(config)? else {
        // Nothing to take out of a config that is not there; a rule needs a file to live in.
        return match executable {
            Some(executable) => write_opencode_config(config, executable),
            None => Ok(()),
        };
    };
    let mut root: Map<String, Value> = if text.trim().is_empty() {
        Map::new()
    } else {
        match serde_json::from_str::<Value>(&text) {
            Ok(Value::Object(root)) => root,
            _ => return Err(invalid_config(config)),
        }
    };
    let before = root.clone();
    set_opencode_rule(&mut root, executable);
    if root != before {
        write_json(config, &root)?;
    }
    Ok(())
}

fn write_opencode_config(config: &Path, executable: &Path) -> Result<(), HookInstallError> {
    let mut root = Map::new();
    root.insert(
        "$schema".to_owned(),
        json!("https://opencode.ai/config.json"),
    );
    set_opencode_rule(&mut root, Some(executable));
    write_json(config, &root)
}

fn set_opencode_rule(root: &mut Map<String, Value>, executable: Option<&Path>) {
    let rule = executable.map(|executable| format!("{}{OPENCODE_RULE_END}", executable.display()));
    // The bash permission may be one answer for every command, or a map of patterns to answers.
    let current = root
        .get("permission")
        .and_then(|permission| permission.get("bash"))
        .cloned();
    let mut patterns = match current {
        Some(Value::Object(map)) => map,
        Some(answer) => {
            let mut map = Map::new();
            map.insert("*".to_owned(), answer);
            map
        }
        None => Map::new(),
    };
    patterns.retain(|pattern, _| !pattern.ends_with(OPENCODE_RULE_END));
    if let Some(rule) = rule {
        patterns.insert(rule, json!("allow"));
    }
    let permission = root.entry("permission").or_insert_with(|| json!({}));
    if let Some(permission) = permission.as_object_mut() {
        if patterns.is_empty() {
            permission.remove("bash");
        } else {
            permission.insert("bash".to_owned(), Value::Object(patterns));
        }
        if permission.is_empty() {
            root.remove("permission");
        }
    }
}

fn write_json(path: &Path, root: &Map<String, Value>) -> Result<(), HookInstallError> {
    let mut text = serde_json::to_string_pretty(root).map_err(|_| invalid_config(path))?;
    text.push('\n');
    write_atomic(path, &text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_codex_rule_allows_the_cli_by_its_executable_and_nothing_else() {
        let text = codex_rules(Path::new("/opt/lynx/lynx-ide"));
        assert!(
            text.contains("pattern = [\"/opt/lynx/lynx-ide\", \"--cli\"]"),
            "{text}"
        );
        assert!(text.contains("decision = \"allow\""));
    }

    #[test]
    fn codex_rules_are_written_in_their_own_file_and_removed_alone() {
        let dir = tempfile::tempdir().expect("temp dir");
        let rules = dir.path().join("rules");
        std::fs::create_dir_all(&rules).expect("folder");
        std::fs::write(
            rules.join("default.rules"),
            "prefix_rule(pattern=[\"git\"], decision=\"prompt\")\n",
        )
        .expect("write");

        write_codex_rule(dir.path(), Some(Path::new("/opt/lynx/lynx-ide"))).expect("write");
        assert!(rules.join(CODEX_RULES_FILE).exists());

        write_codex_rule(dir.path(), None).expect("remove");
        assert!(!rules.join(CODEX_RULES_FILE).exists());
        assert!(
            rules.join("default.rules").exists(),
            "the person's own rules stay"
        );
        write_codex_rule(dir.path(), None).expect("removing twice is fine");
    }

    #[test]
    fn opencode_keeps_the_persons_patterns_and_replaces_only_its_own() {
        let mut root = json!({
            "permission": { "bash": { "*": "ask", "git *": "allow", "/old/lynx-ide --cli *": "allow" } }
        })
        .as_object()
        .cloned()
        .expect("object");

        set_opencode_rule(&mut root, Some(Path::new("/opt/lynx/lynx-ide")));

        let bash = &root["permission"]["bash"];
        assert_eq!(bash["*"], "ask");
        assert_eq!(bash["git *"], "allow");
        assert_eq!(bash["/opt/lynx/lynx-ide --cli *"], "allow");
        assert!(
            bash.get("/old/lynx-ide --cli *").is_none(),
            "an earlier path is replaced"
        );
    }

    #[test]
    fn a_single_bash_answer_becomes_the_fallback_when_lynx_adds_its_pattern() {
        let mut root = json!({ "permission": { "bash": "ask" } })
            .as_object()
            .cloned()
            .expect("object");

        set_opencode_rule(&mut root, Some(Path::new("/opt/lynx/lynx-ide")));

        assert_eq!(root["permission"]["bash"]["*"], "ask");
        assert_eq!(
            root["permission"]["bash"]["/opt/lynx/lynx-ide --cli *"],
            "allow"
        );
    }

    #[test]
    fn removing_the_opencode_rule_leaves_the_file_as_it_was() {
        let original = json!({ "model": "x", "permission": { "bash": { "git *": "allow" } } });
        let mut root = original.as_object().cloned().expect("object");

        set_opencode_rule(&mut root, Some(Path::new("/opt/lynx/lynx-ide")));
        set_opencode_rule(&mut root, None);

        assert_eq!(Value::Object(root), original);
    }

    #[test]
    fn an_opencode_config_that_is_not_json_is_refused_and_not_overwritten() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("opencode.json");
        std::fs::write(&file, "não é json").expect("write");

        assert!(write_opencode_rule(&file, Some(Path::new("/opt/lynx/lynx-ide"))).is_err());
        assert_eq!(std::fs::read_to_string(&file).expect("read"), "não é json");
    }
}
