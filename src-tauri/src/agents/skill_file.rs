//! The `lynx` skill: how an agent reaches Lynx's tools from its shell. Each agent gets the same file in the
//! folder its own harness reads skills from, so the tool list is written once and the path is the only thing
//! that differs.

use std::fs;
use std::io;
use std::path::Path;

use serde_json::Value;

use super::tool_catalog;

pub const NAME: &str = "lynx";

/// The skill an agent reads: the tools it has, with the app's own executable as the command.
pub fn agent_skill(executable: &Path) -> String {
    render(
        executable,
        AGENT_PURPOSE,
        &tool_catalog::list(),
        &tool_catalog::instructions(),
    )
}

/// What an agent uses the skill for, in the description the harness shows.
pub const AGENT_PURPOSE: &str =
    "to start and follow the workers of your team, and to drive the Lynx browser";

/// What a bot uses the skill for. A bot has no browser, so the description does not name one.
pub const BOT_PURPOSE: &str = "to ask the person for permissions, run the commands they allowed, keep your memory, and start and follow the workers of your team";

/// The skill text for `tools`. The description is one line, which the harnesses require.
pub fn render(executable: &Path, purpose: &str, tools: &[Value], instructions: &str) -> String {
    let exe = executable.display();
    let mut text = format!(
        "---\nname: {NAME}\ndescription: Use the Lynx tools from your shell {purpose}. Run \"{exe}\" --cli list to see them.\n---\n\n# Lynx\n\nCall a tool from your shell, with its arguments as JSON:\n\n    \"{exe}\" --cli <tool> '<arguments as JSON>'\n\n- \"{exe}\" --cli list shows the tools you have, with their arguments.\n- Pass - as the arguments to read them from stdin, when they have many quotes.\n- A screenshot is saved to a file and its path is printed: open it with your file tools.\n- A failing call says why on stderr and exits with 1. Read the reason and adjust before you try again.\n\n{instructions}\n\n## Tools\n"
    );
    for tool in tools {
        let name = tool.get("name").and_then(Value::as_str).unwrap_or_default();
        let description = tool
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or_default();
        text.push_str(&format!(
            "\n### {name}\n\n{description}\n\nArguments: {}\n",
            arguments_of(tool)
        ));
    }
    text
}

/// The arguments of a tool as one line: name, type, and whether they are required.
fn arguments_of(tool: &Value) -> String {
    let schema = tool.get("inputSchema");
    let required: Vec<&str> = schema
        .and_then(|schema| schema.get("required"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let arguments: Vec<String> = schema
        .and_then(|schema| schema.get("properties"))
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .map(|(name, property)| {
            let kind = property
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("any");
            let need = if required.contains(&name.as_str()) {
                "required"
            } else {
                "optional"
            };
            format!("{name} ({kind}, {need})")
        })
        .collect();
    if arguments.is_empty() {
        "none".to_owned()
    } else {
        arguments.join(", ")
    }
}

/// Writes the skill into the agent's skills folder, replacing an older copy of it.
pub fn write(skills_dir: &Path, text: &str) -> io::Result<()> {
    let folder = skills_dir.join(NAME);
    fs::create_dir_all(&folder)?;
    let temporary = folder.join("SKILL.md.tmp");
    fs::write(&temporary, text)?;
    fs::rename(temporary, folder.join("SKILL.md"))
}

/// Takes the skill out of the agent's skills folder. A skill that is not there is not an error.
pub fn remove(skills_dir: &Path) -> io::Result<()> {
    match fs::remove_dir_all(skills_dir.join(NAME)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;

    fn tools() -> Vec<Value> {
        vec![json!({
            "name": "lynx_worker_start",
            "description": "Starts a worker.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "taskId": { "type": "string" },
                    "agent": { "type": "string" }
                },
                "required": ["taskId"]
            }
        })]
    }

    #[test]
    fn the_skill_names_the_command_and_every_tool_with_its_arguments() {
        let text = render(
            &PathBuf::from("/opt/lynx/lynx-ide"),
            AGENT_PURPOSE,
            &tools(),
            "",
        );
        assert!(text.starts_with("---\nname: lynx\n"));
        assert!(text.contains("\"/opt/lynx/lynx-ide\" --cli <tool>"));
        assert!(text.contains("### lynx_worker_start"));
        assert!(text.contains("Arguments: taskId (string, required), agent (string, optional)"));
    }

    #[test]
    fn the_description_is_one_line() {
        let text = render(&PathBuf::from("/opt/lynx/lynx-ide"), AGENT_PURPOSE, &[], "");
        let description = text
            .lines()
            .find(|line| line.starts_with("description:"))
            .expect("a description line");
        assert!(description.contains("--cli list"));
    }

    #[test]
    fn a_tool_without_arguments_says_none() {
        let text = render(
            &PathBuf::from("/x"),
            AGENT_PURPOSE,
            &[
                json!({ "name": "lynx_check", "description": "d", "inputSchema": { "type": "object", "properties": {} } }),
            ],
            "",
        );
        assert!(text.contains("Arguments: none"));
    }

    #[test]
    fn writing_replaces_the_copy_and_removing_it_is_safe_twice() {
        let dir = tempfile::tempdir().expect("temp dir");
        let skills = dir.path().join("skills");
        write(&skills, "first").expect("write");
        write(&skills, "second").expect("write again");
        assert_eq!(
            fs::read_to_string(skills.join("lynx/SKILL.md")).expect("file"),
            "second"
        );
        assert!(!skills.join("lynx/SKILL.md.tmp").exists());
        remove(&skills).expect("remove");
        assert!(!skills.join("lynx").exists());
        remove(&skills).expect("removing a missing skill is not an error");
    }
}
