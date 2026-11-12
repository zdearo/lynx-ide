//! The `lynx` skill: how an agent reaches Lynx's tools from its shell. Each agent gets the same file in the
//! folder its own harness reads skills from, so the tool list is written once and the path is the only thing
//! that differs.

use std::fs;
use std::io;
use std::path::Path;

use serde_json::Value;

use super::tool_catalog;

pub const NAME: &str = "lynx";

/// The flows the tool descriptions only name: a team in one call, the asks and their wait, the effort, and how a
/// delivery closes.
const WORKFLOW: &str = r#"## Building a team

Add a plan in one `lynx_member_add` call with `members`, a list. Each member takes `role` and `task` (both required), `acceptance`, `key` (a name inside this list) and `dependsOn` (ids of members already in the team, or keys of members of this list). A member that should start by itself also takes `start: true` and `agent`. The whole list is checked before any member is written: if one is wrong, none is added. Use `parentId` only when you orchestrate in several worktrees.

- `agent` is `claude`, `codex`, `opencode` or `omp`. Ask the person which agent to use, and which model and effort if they care, unless they already said.
- `start: true` starts the worker in the team's worktree as soon as every member it waits for reports done, or at once if they are done. A `failed` report releases nothing. It launches with the agent's own model and effort: to choose them, leave `start` out and call `lynx_worker_start`.
- Without `start` the member stays pending until you call `lynx_worker_start`.

## Starting and changing workers

`lynx_worker_start` (`memberId`, `agent`, `worktree`: `new` or `current`, optional `model` and `effort`) and `lynx_worker_replace` (`memberId`, `agent`, optional `model` and `effort`) take `effort` (`low`, `medium`, `high`, `xhigh` or `max`) when the person asked for a level. Claude and Codex take it; OpenCode and OMP ignore it. Leave it out to keep the agent's own.

## Asking a worker

`lynx_ask` with `memberId` and `text` types the text into a running worker and returns at once. Add `waitSeconds` (0 to 540) and the call waits: it types the text once the worker is free and the person has no draft in its input, then blocks until the worker reports or its turn ends. With `waitSeconds` a text may have several lines; without it, one line. It returns `report` (null when the turn ended without one), `output` (the end of the worker's screen), `running` and `status`.

If the call fails, read the message:
- Not typed: the worker stayed busy, or had a draft, for the whole wait. Nothing was sent.
- Typed, no answer: the worker has the text and did not answer in time.

In both cases read the worker with `lynx_worker_read` before you ask again.

`batch` (a list of `memberId` and `text`) sends one text to each of several workers and does not wait. Do not combine it with `waitSeconds`.

## Asking a member

`lynx_member_ask` (`memberId`: a teammate's id or role; `text`; `waitSeconds`, 540 by default) asks a teammate of your own team and waits the same way. The teammate gets the question in its terminal and answers it with `lynx_report`; the answer comes back to you. The call is refused at once, without waiting, when the teammate is outside your team, is you, has no worker, or already waits for you, directly or through others: neither could answer the other. The question and the answer reach your orchestrator at its next `lynx_check`.

## Timeouts

A call with `waitSeconds` can take up to 540 seconds, and the transport waits 30 seconds longer than the call asks. Give the shell call a timeout of at least 570000 ms, or the shell ends it before Lynx answers.

## Closing a delivery

When every worker has reported and you have read the reports, close the delivery yourself. Lynx runs no lint, test or commit for you.

1. Run the project's lint. Its command is in the project's `AGENTS.md` or `CLAUDE.md`, or the person names it. Fix what it reports before you go on.
2. Commit only when the person asked for a commit. If a worker had its own worktree (`worktree: new`), merge it with `lynx_worker_land` first, also only when asked. Then add a member with the role `Commit`, `start: true` and `agent`, whose `dependsOn` lists the members whose work it commits. Its task lists the exact files to commit, one per line, and says to commit nothing else. No `git add .`.
3. Check the commit with `git log -1 --stat`: the message and the files must be the ones you listed.
4. Tell the person what was done, what was committed, and what is left.
"#;

/// The skill an agent reads: the tools it has, with the app's own executable as the command.
pub fn agent_skill(executable: &Path) -> String {
    render(
        executable,
        AGENT_PURPOSE,
        &tool_catalog::list(),
        &format!("{}\n\n{WORKFLOW}", tool_catalog::instructions()),
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

    #[test]
    fn the_agent_skill_teaches_the_team_the_asks_the_effort_and_the_closing() {
        let text = agent_skill(&PathBuf::from("/opt/lynx/lynx-ide"));
        for topic in [
            "`members`",
            "`key`",
            "`dependsOn`",
            "`start: true`",
            "`lynx_ask`",
            "`waitSeconds`",
            "`lynx_member_ask`",
            "`effort`",
            "570000 ms",
            "`Commit`",
            "git log -1 --stat",
            "Lynx runs no lint",
        ] {
            assert!(text.contains(topic), "the skill teaches {topic}");
        }
    }

    #[test]
    fn every_tool_the_skill_names_exists() {
        let names: Vec<String> = tool_catalog::list()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap_or_default().to_owned())
            .collect();
        for word in WORKFLOW
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .filter(|word| word.starts_with("lynx_"))
        {
            assert!(names.iter().any(|name| name == word), "{word} is a tool");
        }
    }
}
