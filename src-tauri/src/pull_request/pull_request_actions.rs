//! What can be done to a pull request from Lynx: merge it, close it, reopen it, mark it ready or
//! back to draft. Everything goes through the user's `gh`.

use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use specta::Type;

use super::github_remote::{GitHubRemoteError, run_gh};

/// A merge can wait on the network and on GitHub's own checks.
const ACTION_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum MergeMethod {
    Merge,
    Squash,
    Rebase,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PullRequestAction {
    /// With `auto`, GitHub merges it by itself once its requirements pass.
    Merge {
        method: MergeMethod,
        auto: bool,
    },
    Close,
    Reopen,
    /// Takes a draft out of draft.
    MarkReady,
    /// Puts an open pull request back to draft.
    MarkDraft,
}

/// The arguments for `gh pr`. The number is given, so the branch checked out does not matter.
pub fn action_args(number: u32, action: &PullRequestAction) -> Vec<String> {
    let number = number.to_string();
    match action {
        PullRequestAction::Merge { method, auto } => {
            let flag = match method {
                MergeMethod::Merge => "--merge",
                MergeMethod::Squash => "--squash",
                MergeMethod::Rebase => "--rebase",
            };
            let mut args = vec!["pr".to_owned(), "merge".to_owned(), number, flag.to_owned()];
            if *auto {
                args.push("--auto".to_owned());
            }
            args
        }
        PullRequestAction::Close => vec!["pr".to_owned(), "close".to_owned(), number],
        PullRequestAction::Reopen => vec!["pr".to_owned(), "reopen".to_owned(), number],
        PullRequestAction::MarkReady => vec!["pr".to_owned(), "ready".to_owned(), number],
        PullRequestAction::MarkDraft => vec![
            "pr".to_owned(),
            "ready".to_owned(),
            number,
            "--undo".to_owned(),
        ],
    }
}

pub fn run_action(
    root: &Path,
    number: u32,
    action: &PullRequestAction,
) -> Result<(), GitHubRemoteError> {
    run_gh(&action_args(number, action), Some(root), ACTION_TIMEOUT)?;
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawMethods {
    merge_commit_allowed: bool,
    squash_merge_allowed: bool,
    rebase_merge_allowed: bool,
}

/// The merge methods the repository allows, in the order Lynx offers them.
pub fn parse_methods(json: &str) -> Result<Vec<MergeMethod>, GitHubRemoteError> {
    let raw: RawMethods = serde_json::from_str(json).map_err(|_| {
        GitHubRemoteError::Failed("gh answered with something unreadable".to_owned())
    })?;
    let mut methods = Vec::new();
    if raw.squash_merge_allowed {
        methods.push(MergeMethod::Squash);
    }
    if raw.merge_commit_allowed {
        methods.push(MergeMethod::Merge);
    }
    if raw.rebase_merge_allowed {
        methods.push(MergeMethod::Rebase);
    }
    Ok(methods)
}

pub fn merge_methods(root: &Path) -> Result<Vec<MergeMethod>, GitHubRemoteError> {
    let json = run_gh(
        &[
            "repo",
            "view",
            "--json",
            "mergeCommitAllowed,squashMergeAllowed,rebaseMergeAllowed",
        ],
        Some(root),
        Duration::from_secs(30),
    )?;
    parse_methods(&json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_arguments_carry_the_method_and_auto_only_when_asked() {
        let squash = action_args(
            42,
            &PullRequestAction::Merge {
                method: MergeMethod::Squash,
                auto: false,
            },
        );
        assert_eq!(squash, ["pr", "merge", "42", "--squash"]);

        let auto = action_args(
            42,
            &PullRequestAction::Merge {
                method: MergeMethod::Rebase,
                auto: true,
            },
        );
        assert_eq!(auto, ["pr", "merge", "42", "--rebase", "--auto"]);
    }

    #[test]
    fn the_other_actions_map_to_their_gh_commands() {
        assert_eq!(
            action_args(7, &PullRequestAction::Close),
            ["pr", "close", "7"]
        );
        assert_eq!(
            action_args(7, &PullRequestAction::Reopen),
            ["pr", "reopen", "7"]
        );
        assert_eq!(
            action_args(7, &PullRequestAction::MarkReady),
            ["pr", "ready", "7"]
        );
        assert_eq!(
            action_args(7, &PullRequestAction::MarkDraft),
            ["pr", "ready", "7", "--undo"]
        );
    }

    #[test]
    fn only_the_methods_the_repository_allows_are_offered_squash_first() {
        let json = r#"{"mergeCommitAllowed": true, "squashMergeAllowed": true, "rebaseMergeAllowed": false}"#;

        assert_eq!(
            parse_methods(json).expect("methods"),
            [MergeMethod::Squash, MergeMethod::Merge]
        );
        assert!(parse_methods("nope").is_err());
    }
}
