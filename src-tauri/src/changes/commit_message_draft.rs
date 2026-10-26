//! A commit message drafted by the user's own agent CLI, run read-only over the staged diff.
//! Lynx does not call a model API: it asks whichever supported CLI is installed.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use serde::Serialize;
use specta::Type;

use crate::agents::agent_catalog::on_path;
use crate::config::app_config::CommitMessageAgent;
use crate::git::git_command::git;
use crate::process::app_image_environment;
use crate::process::bounded_command::{BoundedError, run_bounded};

use super::commit_message_prompt::build_prompt;

const TIMEOUT: Duration = Duration::from_secs(120);
/// A huge change would make the prompt slow to send and hard for the agent to read.
pub const DIFF_LIMIT_BYTES: usize = 200_000;
/// The subject line git shows in `git log --oneline`; longer ones get cut.
const SUBJECT_CHARS: usize = 72;

#[derive(Debug, thiserror::Error)]
pub enum DraftError {
    #[error("no agent CLI was found on PATH: install Claude Code or Codex")]
    NoAgent,
    #[error("{agent} is not installed or not on PATH; choose another agent in Settings > Git")]
    NotInstalled { agent: &'static str },
    #[error("nothing is staged to draft a message from")]
    NothingStaged,
    #[error("the branch has no commits beyond its base to describe")]
    NothingToCompare,
    #[error("{agent} did not answer within two minutes")]
    TimedOut { agent: &'static str },
    #[error("{agent} failed: {stderr}")]
    Failed { agent: &'static str, stderr: String },
    #[error("{agent} returned no message")]
    Empty { agent: &'static str },
    #[error("could not run {agent}: {source}")]
    Spawn {
        agent: &'static str,
        #[source]
        source: std::io::Error,
    },
}

/// What a generated message is written with: the CLI's name, and its non-interactive flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Agent {
    id: CommitMessageAgent,
    label: &'static str,
    binary: &'static str,
    /// Read-only and single-shot: the prompt arrives on stdin and the answer goes to stdout.
    args: &'static [&'static str],
}

/// In the order Lynx prefers them. All read the prompt from stdin, which takes large diffs.
const AGENTS: [Agent; 3] = [
    Agent {
        id: CommitMessageAgent::Claude,
        label: "Claude Code",
        binary: "claude",
        args: &["-p", "--output-format", "text", "--permission-mode", "plan"],
    },
    Agent {
        id: CommitMessageAgent::Codex,
        label: "Codex",
        binary: "codex",
        args: &[
            "exec",
            "--ephemeral",
            "--skip-git-repo-check",
            "-s",
            "read-only",
        ],
    },
    // OpenCode's `json` format would wrap the answer in events, so the plain text format is asked for.
    Agent {
        id: CommitMessageAgent::Opencode,
        label: "OpenCode",
        binary: "opencode",
        args: &["run", "--format", "default"],
    },
];

/// The agent to ask. `Auto` takes the first installed one. An explicit choice that is not installed
/// is an error: Lynx does not quietly switch to another agent.
fn choose_agent(
    choice: CommitMessageAgent,
    installed: impl Fn(&str) -> bool,
) -> Result<Agent, DraftError> {
    let mut missing = None;
    for agent in AGENTS
        .iter()
        .copied()
        .filter(|agent| choice == CommitMessageAgent::Auto || agent.id == choice)
    {
        if installed(agent.binary) {
            return Ok(agent);
        }
        missing = Some(agent.label);
    }
    match (choice, missing) {
        (CommitMessageAgent::Auto, _) | (_, None) => Err(DraftError::NoAgent),
        (_, Some(agent)) => Err(DraftError::NotInstalled { agent }),
    }
}

/// A message the user can edit before committing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DraftedMessage {
    pub summary: String,
    pub description: String,
    /// Which CLI wrote it, shown to the user.
    pub agent_label: String,
}

/// Cuts a diff to `limit` bytes at a line break, never inside a character.
pub fn truncate_diff(diff: String, limit: usize) -> String {
    if diff.len() <= limit {
        return diff;
    }
    let mut end = limit;
    while !diff.is_char_boundary(end) {
        end -= 1;
    }
    let cut = diff[..end].rfind('\n').unwrap_or(end);
    format!("{}\n[diff truncated]", &diff[..cut])
}

/// Splits the agent's answer into summary and description. Strips a code fence around it.
/// Returns `None` when nothing is left to use as a summary.
fn parse_message(raw: &str) -> Option<(String, String)> {
    parse_titled(raw, SUBJECT_CHARS)
}

/// Like `parse_message`, with the title cut at `max_title_chars`. A pull request title may be longer
/// than a commit subject.
pub fn parse_titled(raw: &str, max_title_chars: usize) -> Option<(String, String)> {
    let mut lines: Vec<&str> = raw.trim().lines().collect();
    if lines
        .first()
        .is_some_and(|line| line.trim_start().starts_with("```"))
    {
        lines.remove(0);
        if lines.last().is_some_and(|line| line.trim() == "```") {
            lines.pop();
        }
    }
    let text = lines.join("\n");
    let text = text.trim();
    let (first, rest) = text.split_once('\n').unwrap_or((text, ""));
    let summary: String = first
        .trim()
        .trim_end_matches('.')
        .trim()
        .chars()
        .take(max_title_chars)
        .collect();
    let summary = summary.trim_end().to_owned();
    if summary.is_empty() {
        return None;
    }
    Some((summary, rest.trim().to_owned()))
}

/// Runs the agent with the prompt on stdin, and returns what it wrote to stdout.
fn run_agent(
    root: &Path,
    agent: Agent,
    prompt: &str,
    timeout: Duration,
) -> Result<String, DraftError> {
    let mut command = if cfg!(windows) {
        // A Windows CLI is usually a `.cmd` shim, which only the shell can start.
        let mut command = Command::new("cmd");
        command.args(["/C", agent.binary]);
        command
    } else {
        Command::new(agent.binary)
    };
    command.args(agent.args).current_dir(root);
    app_image_environment::clean_command(&mut command);
    let output =
        run_bounded(&mut command, prompt.as_bytes(), timeout).map_err(|error| match error {
            BoundedError::TimedOut => DraftError::TimedOut { agent: agent.label },
            BoundedError::Spawn(source) if source.kind() == std::io::ErrorKind::NotFound => {
                DraftError::NoAgent
            }
            BoundedError::Spawn(source) => DraftError::Spawn {
                agent: agent.label,
                source,
            },
        })?;

    if !output.status.success() {
        return Err(DraftError::Failed {
            agent: agent.label,
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Drafts a message for what is staged. `style` is the instruction text; the app adds its own
/// output rules after it.
pub fn draft_message(
    root: &Path,
    choice: CommitMessageAgent,
    style: &str,
) -> Result<DraftedMessage, DraftError> {
    let diff = git(root)
        .args(["diff", "--cached", "--no-color", "--no-ext-diff"])
        .run_bytes()
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .map_err(|_| DraftError::NothingStaged)?;
    if diff.trim().is_empty() {
        return Err(DraftError::NothingStaged);
    }

    let prompt = build_prompt(style, &truncate_diff(diff, DIFF_LIMIT_BYTES));
    let (label, answer) = ask_agent(root, choice, &prompt)?;
    let (summary, description) =
        parse_message(&answer).ok_or(DraftError::Empty { agent: label })?;
    Ok(DraftedMessage {
        summary,
        description,
        agent_label: label.to_owned(),
    })
}

/// Runs the chosen agent CLI on a prompt, and returns its label with the raw answer.
pub fn ask_agent(
    root: &Path,
    choice: CommitMessageAgent,
    prompt: &str,
) -> Result<(&'static str, String), DraftError> {
    let agent = choose_agent(choice, on_path)?;
    let answer = run_agent(root, agent, prompt, TIMEOUT)?;
    Ok((agent.label, answer))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_takes_the_first_installed_agent() {
        let codex_only = choose_agent(CommitMessageAgent::Auto, |binary| binary == "codex");
        assert_eq!(codex_only.expect("codex").binary, "codex");

        let both = choose_agent(CommitMessageAgent::Auto, |_| true);
        assert_eq!(both.expect("claude").binary, "claude");

        assert!(matches!(
            choose_agent(CommitMessageAgent::Auto, |_| false),
            Err(DraftError::NoAgent)
        ));
    }

    #[test]
    fn an_explicit_agent_is_used_or_reported_missing_and_never_swapped() {
        let codex = choose_agent(CommitMessageAgent::Codex, |binary| binary == "codex");
        assert_eq!(codex.expect("codex").binary, "codex");

        assert!(matches!(
            choose_agent(CommitMessageAgent::Codex, |binary| binary == "claude"),
            Err(DraftError::NotInstalled { agent: "Codex" })
        ));
    }

    #[test]
    fn opencode_is_chosen_by_name_and_reported_by_its_label() {
        let opencode = choose_agent(CommitMessageAgent::Opencode, |binary| binary == "opencode");
        assert_eq!(opencode.expect("opencode").binary, "opencode");

        assert!(matches!(
            choose_agent(CommitMessageAgent::Opencode, |_| false),
            Err(DraftError::NotInstalled { agent: "OpenCode" })
        ));
    }

    #[test]
    fn a_plain_answer_splits_into_summary_and_body() {
        assert_eq!(
            parse_message("Fix the login label.\n\nThe label was stale.\n"),
            Some((
                "Fix the login label".to_owned(),
                "The label was stale.".to_owned()
            ))
        );
    }

    #[test]
    fn a_fenced_answer_loses_the_fence() {
        assert_eq!(
            parse_message("```\nAdd tests\n```"),
            Some(("Add tests".to_owned(), String::new()))
        );
    }

    #[test]
    fn a_long_summary_is_cut_to_the_subject_length() {
        let (summary, _) = parse_message(&"a".repeat(100)).expect("summary");
        assert_eq!(summary.chars().count(), SUBJECT_CHARS);
    }

    #[test]
    fn an_empty_answer_gives_no_message() {
        assert_eq!(parse_message("  \n\n "), None);
    }

    #[test]
    fn a_long_diff_is_cut_on_a_line_and_a_character() {
        let diff = "a\nééé\nlast line\n".to_owned();
        // Whole lines that fit are kept.
        assert_eq!(truncate_diff(diff.clone(), 9), "a\nééé\n[diff truncated]");
        // A line whose break does not fit is dropped, not cut in half.
        assert_eq!(truncate_diff(diff.clone(), 8), "a\n[diff truncated]");
        // A limit inside a two-byte character never splits it.
        assert_eq!(truncate_diff(diff.clone(), 3), "a\n[diff truncated]");
        assert_eq!(truncate_diff(diff.clone(), 1000), diff);
    }

    #[cfg(unix)]
    #[test]
    fn the_prompt_reaches_the_agent_and_its_answer_comes_back() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cat = Agent {
            id: CommitMessageAgent::Auto,
            label: "cat",
            binary: "/bin/cat",
            args: &[],
        };

        let answer =
            run_agent(dir.path(), cat, "hello agent", Duration::from_secs(10)).expect("run");

        assert_eq!(answer, "hello agent");
    }

    #[cfg(unix)]
    #[test]
    fn an_agent_that_takes_too_long_is_stopped() {
        let dir = tempfile::tempdir().expect("temp dir");
        let sleep = Agent {
            id: CommitMessageAgent::Auto,
            label: "sleep",
            binary: "/bin/sleep",
            args: &["5"],
        };

        let error =
            run_agent(dir.path(), sleep, "", Duration::from_millis(200)).expect_err("timed out");

        assert!(matches!(error, DraftError::TimedOut { .. }));
    }
}
