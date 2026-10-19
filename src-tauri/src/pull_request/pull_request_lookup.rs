//! `gh pr view` for the current branch, and what its answer means.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::bounded_command::{BoundedError, run_bounded};

const TIMEOUT: Duration = Duration::from_secs(30);
const FIELDS: &str = "number,title,url,state,isDraft,reviewDecision,headRefName,statusCheckRollup";

#[derive(Debug, thiserror::Error)]
pub enum PullRequestError {
    #[error("the GitHub CLI (gh) is not installed or not on PATH")]
    NotInstalled,
    #[error("the GitHub CLI is not logged in")]
    NotLoggedIn,
    #[error("this repository has no GitHub remote")]
    NoRemote,
    #[error("gh did not answer in time")]
    TimedOut,
    #[error("gh failed: {0}")]
    Failed(String),
    #[error("gh answered with something Lynx cannot read")]
    Unreadable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum PullRequestState {
    Open,
    Closed,
    Merged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ReviewDecision {
    Approved,
    ChangesRequested,
    ReviewRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum CheckState {
    Passed,
    Failed,
    Pending,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestCheck {
    /// `workflow / job` for a GitHub Actions job, or the context name for a commit status.
    pub name: String,
    pub state: CheckState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PullRequest {
    pub number: u32,
    pub title: String,
    pub url: String,
    pub state: PullRequestState,
    pub is_draft: bool,
    /// `None` when nobody has reviewed it yet, or the repository does not require reviews.
    pub review: Option<ReviewDecision>,
    pub head_branch: String,
    pub checks: Vec<PullRequestCheck>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPullRequest {
    number: u32,
    title: String,
    url: String,
    state: String,
    is_draft: bool,
    review_decision: Option<String>,
    head_ref_name: String,
    #[serde(default)]
    status_check_rollup: Vec<RawCheck>,
}

/// A check run (`status` and `conclusion`) or a commit status (`state`), in the same list.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCheck {
    name: Option<String>,
    context: Option<String>,
    workflow_name: Option<String>,
    status: Option<String>,
    conclusion: Option<String>,
    state: Option<String>,
}

/// The pull request of the branch checked out in `root`, or `None` when the branch has none.
pub fn lookup(root: &Path) -> Result<Option<PullRequest>, PullRequestError> {
    let mut command = Command::new("gh");
    command
        .args(["pr", "view", "--json", FIELDS])
        .current_dir(root)
        // Without this, gh would stop to ask questions that nothing here can answer.
        .env("GH_PROMPT_DISABLED", "1");
    let output = run_bounded(&mut command, &[], TIMEOUT).map_err(|error| match error {
        BoundedError::TimedOut => PullRequestError::TimedOut,
        BoundedError::Spawn(source) if source.kind() == std::io::ErrorKind::NotFound => {
            PullRequestError::NotInstalled
        }
        BoundedError::Spawn(source) => PullRequestError::Failed(source.to_string()),
    })?;

    if output.status.success() {
        return parse_pull_request(&String::from_utf8_lossy(&output.stdout)).map(Some);
    }
    classify_failure(&String::from_utf8_lossy(&output.stderr)).map(|()| None)
}

/// What a failed `gh` run means. `Ok(())` is "no pull request for this branch": an answer, not an error.
pub fn classify_failure(stderr: &str) -> Result<(), PullRequestError> {
    let text = stderr.to_lowercase();
    if text.contains("no pull requests found")
        || text.contains("could not determine current branch")
    {
        return Ok(());
    }
    if text.contains("gh auth login") || text.contains("not logged in") {
        return Err(PullRequestError::NotLoggedIn);
    }
    if text.contains("no git remotes") || text.contains("known github host") {
        return Err(PullRequestError::NoRemote);
    }
    Err(PullRequestError::Failed(stderr.trim().to_owned()))
}

pub fn parse_pull_request(json: &str) -> Result<PullRequest, PullRequestError> {
    let raw: RawPullRequest =
        serde_json::from_str(json).map_err(|_| PullRequestError::Unreadable)?;
    Ok(PullRequest {
        number: raw.number,
        title: raw.title,
        url: raw.url,
        state: state_of(&raw.state)?,
        is_draft: raw.is_draft,
        review: review_of(raw.review_decision.as_deref()),
        head_branch: raw.head_ref_name,
        checks: raw
            .status_check_rollup
            .into_iter()
            .filter_map(check_of)
            .collect(),
    })
}

fn state_of(raw: &str) -> Result<PullRequestState, PullRequestError> {
    match raw {
        "OPEN" => Ok(PullRequestState::Open),
        "CLOSED" => Ok(PullRequestState::Closed),
        "MERGED" => Ok(PullRequestState::Merged),
        _ => Err(PullRequestError::Unreadable),
    }
}

fn review_of(raw: Option<&str>) -> Option<ReviewDecision> {
    match raw? {
        "APPROVED" => Some(ReviewDecision::Approved),
        "CHANGES_REQUESTED" => Some(ReviewDecision::ChangesRequested),
        "REVIEW_REQUIRED" => Some(ReviewDecision::ReviewRequired),
        _ => None,
    }
}

fn check_of(raw: RawCheck) -> Option<PullRequestCheck> {
    let name = match (raw.workflow_name.as_deref(), raw.name.or(raw.context)) {
        (Some(workflow), Some(name)) if !workflow.is_empty() => format!("{workflow} / {name}"),
        (_, Some(name)) => name,
        (_, None) => return None,
    };
    let state = match raw.state.as_deref() {
        Some(status) => commit_status_state(status),
        None => check_run_state(raw.status.as_deref(), raw.conclusion.as_deref()),
    };
    Some(PullRequestCheck { name, state })
}

fn commit_status_state(state: &str) -> CheckState {
    match state {
        "SUCCESS" => CheckState::Passed,
        "FAILURE" | "ERROR" => CheckState::Failed,
        _ => CheckState::Pending,
    }
}

fn check_run_state(status: Option<&str>, conclusion: Option<&str>) -> CheckState {
    if status != Some("COMPLETED") {
        return CheckState::Pending;
    }
    match conclusion {
        Some("SUCCESS") => CheckState::Passed,
        Some("NEUTRAL" | "SKIPPED") => CheckState::Skipped,
        Some(
            "FAILURE" | "TIMED_OUT" | "CANCELLED" | "ACTION_REQUIRED" | "STARTUP_FAILURE" | "STALE",
        ) => CheckState::Failed,
        _ => CheckState::Pending,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DRAFT_JSON: &str = r#"{
        "number": 42,
        "title": "fix(auth): refresh the token",
        "url": "https://github.com/acme/app/pull/42",
        "state": "OPEN",
        "isDraft": true,
        "reviewDecision": "",
        "headRefName": "feat/auth",
        "statusCheckRollup": [
            {"__typename": "CheckRun", "name": "lint", "workflowName": "CI",
             "status": "COMPLETED", "conclusion": "SUCCESS"},
            {"__typename": "CheckRun", "name": "test", "workflowName": "CI",
             "status": "COMPLETED", "conclusion": "FAILURE"},
            {"__typename": "CheckRun", "name": "build", "workflowName": "CI",
             "status": "IN_PROGRESS", "conclusion": ""},
            {"__typename": "CheckRun", "name": "docs", "workflowName": "CI",
             "status": "COMPLETED", "conclusion": "SKIPPED"},
            {"__typename": "StatusContext", "context": "deploy/preview", "state": "PENDING"}
        ]
    }"#;

    #[test]
    fn a_draft_pull_request_maps_its_fields_and_checks() {
        let pr = parse_pull_request(DRAFT_JSON).expect("parse");

        assert_eq!(pr.number, 42);
        assert_eq!(pr.state, PullRequestState::Open);
        assert!(pr.is_draft);
        assert_eq!(pr.review, None);
        assert_eq!(pr.head_branch, "feat/auth");
        let states: Vec<_> = pr
            .checks
            .iter()
            .map(|check| (check.name.as_str(), check.state))
            .collect();
        assert_eq!(
            states,
            [
                ("CI / lint", CheckState::Passed),
                ("CI / test", CheckState::Failed),
                ("CI / build", CheckState::Pending),
                ("CI / docs", CheckState::Skipped),
                ("deploy/preview", CheckState::Pending),
            ]
        );
    }

    #[test]
    fn merged_and_approved_are_read_as_such() {
        let json = r#"{"number": 7, "title": "t", "url": "u", "state": "MERGED",
            "isDraft": false, "reviewDecision": "APPROVED", "headRefName": "b",
            "statusCheckRollup": []}"#;

        let pr = parse_pull_request(json).expect("parse");

        assert_eq!(pr.state, PullRequestState::Merged);
        assert_eq!(pr.review, Some(ReviewDecision::Approved));
        assert!(pr.checks.is_empty());
    }

    #[test]
    fn an_unknown_state_or_broken_json_is_unreadable() {
        let unknown = DRAFT_JSON.replace("\"OPEN\"", "\"SOMETHING\"");
        assert!(matches!(
            parse_pull_request(&unknown),
            Err(PullRequestError::Unreadable)
        ));
        assert!(matches!(
            parse_pull_request("{"),
            Err(PullRequestError::Unreadable)
        ));
    }

    #[test]
    fn a_missing_branch_is_an_answer_and_not_an_error() {
        assert!(classify_failure("no pull requests found for branch \"main\"").is_ok());
        assert!(classify_failure("could not determine current branch: detached").is_ok());
    }

    #[test]
    fn gh_messages_for_login_and_remote_are_named() {
        assert!(matches!(
            classify_failure("To get started with GitHub CLI, please run:  gh auth login"),
            Err(PullRequestError::NotLoggedIn)
        ));
        assert!(matches!(
            classify_failure("no git remotes found"),
            Err(PullRequestError::NoRemote)
        ));
        assert!(matches!(
            classify_failure("boom"),
            Err(PullRequestError::Failed(text)) if text == "boom"
        ));
    }

    #[test]
    fn a_commit_status_and_a_check_run_read_their_own_fields() {
        assert_eq!(commit_status_state("ERROR"), CheckState::Failed);
        assert_eq!(check_run_state(Some("QUEUED"), None), CheckState::Pending);
        assert_eq!(
            check_run_state(Some("COMPLETED"), Some("NEUTRAL")),
            CheckState::Skipped
        );
    }
}
