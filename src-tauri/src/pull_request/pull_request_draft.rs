//! A pull request title and description written by the user's agent CLI from the branch's commits
//! and its diff against the base.

use std::path::Path;

use serde::Serialize;
use specta::Type;

use crate::changes::branch_sync;
use crate::changes::commit_message_draft::{
    DIFF_LIMIT_BYTES, DraftError, ask_agent, parse_titled, truncate_diff,
};
use crate::changes::commit_message_prompt::build_pull_request_prompt;
use crate::config::app_config::CommitMessageAgent;
use crate::error::AppError;
use crate::git::git_command::git;

use super::pull_request_create::{CreatePullRequestError, is_valid_base};

/// A pull request title is read on one line of a list, so a bit more room than a commit subject.
const TITLE_CHARS: usize = 100;
const MAX_COMMITS: &str = "30";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DraftedPullRequest {
    pub title: String,
    pub body: String,
    /// Which CLI wrote it, shown to the user.
    pub agent_label: String,
}

/// Drafts the pull request of the current branch into `base`. `style` is the title style text.
pub fn draft(
    root: &Path,
    choice: CommitMessageAgent,
    style: &str,
    base: &str,
) -> Result<DraftedPullRequest, AppError> {
    if !is_valid_base(base) {
        return Err(CreatePullRequestError::Invalid("the base branch name is not valid").into());
    }
    let sync = branch_sync::read(root)?;
    let Some(remote) = sync.remote else {
        return Err(AppError::PushUnavailable(
            "there is no remote to compare with",
        ));
    };
    let range = format!("{remote}/{base}");

    let commits = git(root)
        .args([
            "log",
            "--format=- %s (%h)",
            "-n",
            MAX_COMMITS,
            &format!("{range}..HEAD"),
        ])
        .run()?;
    if commits.trim().is_empty() {
        return Err(DraftError::NothingToCompare.into());
    }
    let diff = git(root)
        .args([
            "diff",
            "--no-color",
            "--no-ext-diff",
            &format!("{range}...HEAD"),
        ])
        .run_bytes()
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())?;

    let prompt = build_pull_request_prompt(style, &commits, &truncate_diff(diff, DIFF_LIMIT_BYTES));
    let (label, answer) = ask_agent(root, choice, &prompt)?;
    let (title, body) =
        parse_titled(&answer, TITLE_CHARS).ok_or(DraftError::Empty { agent: label })?;
    Ok(DraftedPullRequest {
        title,
        body,
        agent_label: label.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::test_repository::TestRepository;

    #[test]
    fn a_branch_with_nothing_beyond_its_base_is_refused_before_any_agent_runs() {
        let repo = TestRepository::new().with_origin();

        let error = draft(&repo.path, CommitMessageAgent::Auto, "style", "main")
            .expect_err("nothing to compare");

        assert!(matches!(
            error,
            AppError::CommitDraft(DraftError::NothingToCompare)
        ));
    }

    #[test]
    fn a_base_that_could_be_a_flag_is_refused() {
        let repo = TestRepository::new().with_origin();

        assert!(draft(&repo.path, CommitMessageAgent::Auto, "style", "--output=x").is_err());
    }
}
