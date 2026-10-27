//! The commands a bot may run, as patterns like `minha-cli *`. The person adds them, or approves the
//! ones the bot asks for, and each run hands them to the CLI as `Bash(pattern)` rules. The CLI matches
//! the whole command against them, so a command chained after an allowed one (`;`, `&&`, `$(...)`) or
//! any other command is refused. A pattern that would allow anything is refused here, so the list can
//! never be a shell in disguise.

use std::path::Path;

use super::bot_folder::{self, BotError};

const MAX_PATTERN: usize = 200;
const MAX_PATTERNS: usize = 50;
/// Programs that run whatever they are given, so allowing them allows everything.
const LAUNCHERS: [&str; 17] = [
    "sh", "bash", "zsh", "fish", "dash", "ksh", "csh", "tcsh", "sudo", "su", "doas", "env", "eval",
    "exec", "xargs", "nohup", "command",
];

/// The pattern as it will be kept, or the reason it is refused.
pub fn check(pattern: &str) -> Result<String, BotError> {
    let refused = |why: &str| BotError::InvalidCommand(format!("\"{}\" {why}", pattern.trim()));
    let pattern = pattern.trim();
    if pattern.is_empty() {
        return Err(BotError::InvalidCommand("a pattern is empty".to_owned()));
    }
    if pattern.chars().count() > MAX_PATTERN {
        return Err(refused("is longer than 200 characters"));
    }
    if pattern
        .chars()
        .any(|c| c.is_control() || c == '(' || c == ')')
    {
        return Err(refused("has a line break or a parenthesis"));
    }
    let program = pattern.split_whitespace().next().unwrap_or_default();
    if program.contains('*') {
        return Err(refused("must start with the program, not a wildcard"));
    }
    let name = program.rsplit(['/', '\\']).next().unwrap_or(program);
    if LAUNCHERS.contains(&name.to_lowercase().as_str()) {
        return Err(refused(
            "runs any command it is given, so it would allow everything",
        ));
    }
    Ok(pattern.to_owned())
}

/// The patterns a bot may run.
pub fn allowed(bots_dir: &Path, id: &str) -> Result<Vec<String>, BotError> {
    Ok(bot_folder::BotFolder::existing(bots_dir, id)?
        .settings()?
        .commands)
}

/// Lets a bot run the patterns. One that is refused refuses them all, and one the bot has already
/// changes nothing. Returns the patterns after the change.
pub fn allow(bots_dir: &Path, id: &str, patterns: &[String]) -> Result<Vec<String>, BotError> {
    let checked = patterns
        .iter()
        .map(|pattern| check(pattern))
        .collect::<Result<Vec<_>, _>>()?;
    if checked.is_empty() {
        return Err(BotError::InvalidCommand("no pattern was given".to_owned()));
    }
    let mut full = false;
    bot_folder::edit_settings(bots_dir, id, |settings| {
        for pattern in checked {
            if !settings.commands.contains(&pattern) {
                settings.commands.push(pattern);
            }
        }
        full = settings.commands.len() > MAX_PATTERNS;
        settings.commands.truncate(MAX_PATTERNS);
    })?;
    if full {
        return Err(BotError::InvalidCommand(format!(
            "a bot can have up to {MAX_PATTERNS} patterns"
        )));
    }
    allowed(bots_dir, id)
}

/// Takes a pattern back. Returns the patterns after the change.
pub fn remove(bots_dir: &Path, id: &str, pattern: &str) -> Result<Vec<String>, BotError> {
    if !allowed(bots_dir, id)?.iter().any(|known| known == pattern) {
        return Err(BotError::InvalidCommand(
            "the bot has no such pattern".to_owned(),
        ));
    }
    bot_folder::edit_settings(bots_dir, id, |settings| {
        settings.commands.retain(|known| known != pattern);
    })?;
    allowed(bots_dir, id)
}

/// The rules the CLI is given for these patterns.
pub fn rules(patterns: &[String]) -> Vec<String> {
    patterns
        .iter()
        .map(|pattern| format!("Bash({pattern})"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bots::bot_folder::{NewBot, create};

    fn bot(dir: &Path) -> String {
        create(
            dir,
            NewBot {
                name: "Ada".to_owned(),
                color: "rose".to_owned(),
                provider: "claude".to_owned(),
                model: None,
                instructions: "Ajude.".to_owned(),
                team: None,
            },
        )
        .expect("a bot")
        .id
    }

    #[test]
    fn a_pattern_names_a_program_and_may_end_in_a_wildcard() {
        assert_eq!(check("  minha-cli *  ").expect("ok"), "minha-cli *");
        assert_eq!(check("git status").expect("ok"), "git status");
        assert_eq!(
            check("/usr/local/bin/minha-cli deploy *").expect("ok"),
            "/usr/local/bin/minha-cli deploy *"
        );
    }

    #[test]
    fn a_pattern_that_would_allow_everything_is_refused() {
        for pattern in [
            "",
            "   ",
            "*",
            "* status",
            "bash -c *",
            "sh *",
            "/bin/bash *",
            "SUDO rm *",
            "env *",
            "eval *",
            "xargs *",
            "git (status)",
            "git\nstatus",
        ] {
            assert!(
                matches!(check(pattern), Err(BotError::InvalidCommand(_))),
                "{pattern:?}"
            );
        }
        assert!(check(&format!("tool {}", "x".repeat(MAX_PATTERN))).is_err());
    }

    #[test]
    fn patterns_are_kept_once_listed_and_taken_back() {
        let dir = tempfile::tempdir().expect("temp dir");
        let id = bot(dir.path());

        allow(
            dir.path(),
            &id,
            &["minha-cli *".to_owned(), "git status".to_owned()],
        )
        .expect("allowed");
        let after = allow(dir.path(), &id, &["minha-cli *".to_owned()]).expect("again");
        assert_eq!(after, ["minha-cli *", "git status"]);
        assert_eq!(rules(&after), ["Bash(minha-cli *)", "Bash(git status)"]);

        assert_eq!(
            remove(dir.path(), &id, "git status").expect("removed"),
            ["minha-cli *"]
        );
        assert!(remove(dir.path(), &id, "git status").is_err());
    }

    #[test]
    fn one_bad_pattern_refuses_the_whole_request_and_changes_nothing() {
        let dir = tempfile::tempdir().expect("temp dir");
        let id = bot(dir.path());

        assert!(
            allow(
                dir.path(),
                &id,
                &["minha-cli *".to_owned(), "bash *".to_owned()]
            )
            .is_err()
        );
        assert!(allow(dir.path(), &id, &[]).is_err());
        assert!(allowed(dir.path(), &id).expect("list").is_empty());
    }

    #[test]
    fn a_bot_cannot_have_more_than_the_limit() {
        let dir = tempfile::tempdir().expect("temp dir");
        let id = bot(dir.path());
        let many: Vec<String> = (0..=MAX_PATTERNS).map(|n| format!("tool{n} *")).collect();

        assert!(allow(dir.path(), &id, &many).is_err());
        assert!(allowed(dir.path(), &id).expect("list").len() <= MAX_PATTERNS);
    }
}
