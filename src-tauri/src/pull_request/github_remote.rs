//! Creates the GitHub repository for a local repository that has no remote, through the user's `gh`
//! login, and connects it as `origin`.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::bounded_command::{BoundedError, run_bounded};

const LOOKUP_TIMEOUT: Duration = Duration::from_secs(30);
/// Creating a repository and pushing a branch can take a while on a slow connection.
const CREATE_TIMEOUT: Duration = Duration::from_secs(120);
/// GitHub's own limits: a repository name up to 100 characters, an account name up to 39.
const MAX_NAME_CHARS: usize = 100;
const MAX_OWNER_CHARS: usize = 39;

#[derive(Debug, thiserror::Error)]
pub enum GitHubRemoteError {
    #[error("the GitHub CLI (gh) is not installed or not on PATH")]
    NotInstalled,
    #[error("the GitHub CLI is not logged in")]
    NotLoggedIn,
    #[error("{0}")]
    InvalidRepoName(&'static str),
    #[error("a repository with that name already exists on that account")]
    NameTaken,
    #[error("this repository already has a remote named origin")]
    RemoteExists,
    #[error("gh did not answer in time")]
    TimedOut,
    #[error("gh failed: {0}")]
    Failed(String),
}

/// The account `gh` is signed in with, and the organizations it can create repositories for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GitHubAccount {
    pub login: String,
    pub organizations: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum RepoVisibility {
    Private,
    Public,
}

/// What the user chose in the dialog. `owner` is an account or organization name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateRemoteRequest {
    pub owner: String,
    pub name: String,
    pub visibility: RepoVisibility,
    /// Pushes the current branch, so GitHub knows about it and its pull request can be found.
    pub push: bool,
}

/// The login and organizations of the signed-in account.
pub fn account() -> Result<GitHubAccount, GitHubRemoteError> {
    let login = run_gh(&["api", "user", "--jq", ".login"], None, LOOKUP_TIMEOUT)?;
    // Without the scope to list organizations, the login alone is still enough to create a repository.
    let organizations = run_gh(
        &["api", "user/orgs", "--jq", ".[].login"],
        None,
        LOOKUP_TIMEOUT,
    )
    .map(|text| {
        text.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect()
    })
    .unwrap_or_default();
    Ok(GitHubAccount {
        login: login.trim().to_owned(),
        organizations,
    })
}

/// Creates the repository on GitHub, adds it as `origin` of the repository at `root`, and pushes
/// the current branch when asked.
pub fn create_remote(root: &Path, request: &CreateRemoteRequest) -> Result<(), GitHubRemoteError> {
    validate(request)?;
    run_gh(&create_args(request, root), Some(root), CREATE_TIMEOUT)?;
    Ok(())
}

pub fn validate(request: &CreateRemoteRequest) -> Result<(), GitHubRemoteError> {
    if !is_valid_owner(&request.owner) {
        return Err(GitHubRemoteError::InvalidRepoName(
            "the account name has characters GitHub does not allow",
        ));
    }
    if !is_valid_repo_name(&request.name) {
        return Err(GitHubRemoteError::InvalidRepoName(
            "the repository name has characters GitHub does not allow",
        ));
    }
    Ok(())
}

/// Letters, digits and hyphens; no leading or trailing hyphen; at most 39 characters.
pub fn is_valid_owner(owner: &str) -> bool {
    !owner.is_empty()
        && owner.chars().count() <= MAX_OWNER_CHARS
        && !owner.starts_with('-')
        && !owner.ends_with('-')
        && owner.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// Letters, digits, hyphens, underscores and dots; not `.` or `..`; at most 100 characters.
pub fn is_valid_repo_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().count() <= MAX_NAME_CHARS
        && name != "."
        && name != ".."
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// The arguments for `gh repo create`. `--source` makes it add the remote to the repository at `source`.
pub fn create_args(request: &CreateRemoteRequest, source: &Path) -> Vec<String> {
    let visibility = match request.visibility {
        RepoVisibility::Private => "--private",
        RepoVisibility::Public => "--public",
    };
    let mut args = vec![
        "repo".to_owned(),
        "create".to_owned(),
        format!("{}/{}", request.owner, request.name),
        visibility.to_owned(),
        format!("--source={}", source.display()),
        "--remote=origin".to_owned(),
    ];
    if request.push {
        args.push("--push".to_owned());
    }
    args
}

/// What a failed `gh` run means. Unknown messages are kept as they are, so the user can see them.
pub fn classify_failure(stderr: &str) -> GitHubRemoteError {
    let text = stderr.to_lowercase();
    if text.contains("gh auth login") || text.contains("not logged in") {
        return GitHubRemoteError::NotLoggedIn;
    }
    if text.contains("remote") && text.contains("already exists") {
        return GitHubRemoteError::RemoteExists;
    }
    if text.contains("already exists") {
        return GitHubRemoteError::NameTaken;
    }
    GitHubRemoteError::Failed(stderr.trim().to_owned())
}

pub(super) fn run_gh<S: AsRef<str>>(
    args: &[S],
    dir: Option<&Path>,
    timeout: Duration,
) -> Result<String, GitHubRemoteError> {
    run_gh_with_input(args, dir, &[], timeout)
}

/// Like `run_gh`, with `input` on gh's stdin: where a long comment goes, away from the command line.
pub(super) fn run_gh_with_input<S: AsRef<str>>(
    args: &[S],
    dir: Option<&Path>,
    input: &[u8],
    timeout: Duration,
) -> Result<String, GitHubRemoteError> {
    let mut command = Command::new("gh");
    command
        .args(args.iter().map(AsRef::as_ref))
        // Without this, gh would stop to ask questions that nothing here can answer.
        .env("GH_PROMPT_DISABLED", "1");
    if let Some(dir) = dir {
        command.current_dir(dir);
    }
    let output = run_bounded(&mut command, input, timeout).map_err(|error| match error {
        BoundedError::TimedOut => GitHubRemoteError::TimedOut,
        BoundedError::Spawn(source) if source.kind() == std::io::ErrorKind::NotFound => {
            GitHubRemoteError::NotInstalled
        }
        BoundedError::Spawn(source) => GitHubRemoteError::Failed(source.to_string()),
    })?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    Err(classify_failure(&String::from_utf8_lossy(&output.stderr)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(
        owner: &str,
        name: &str,
        visibility: RepoVisibility,
        push: bool,
    ) -> CreateRemoteRequest {
        CreateRemoteRequest {
            owner: owner.to_owned(),
            name: name.to_owned(),
            visibility,
            push,
        }
    }

    #[test]
    fn account_names_follow_github_rules() {
        assert!(is_valid_owner("zdearo"));
        assert!(is_valid_owner("my-org"));
        assert!(!is_valid_owner(""));
        assert!(!is_valid_owner("-lead"));
        assert!(!is_valid_owner("trail-"));
        assert!(!is_valid_owner("has_underscore"));
        assert!(!is_valid_owner(&"a".repeat(MAX_OWNER_CHARS + 1)));
    }

    #[test]
    fn repository_names_follow_github_rules() {
        assert!(is_valid_repo_name("lynx-ide"));
        assert!(is_valid_repo_name("agent_ade.v2"));
        assert!(!is_valid_repo_name(""));
        assert!(!is_valid_repo_name("."));
        assert!(!is_valid_repo_name(".."));
        assert!(!is_valid_repo_name("with space"));
        assert!(!is_valid_repo_name("slash/inside"));
        assert!(!is_valid_repo_name(&"a".repeat(MAX_NAME_CHARS + 1)));
    }

    #[test]
    fn an_invalid_name_is_refused_before_gh_runs() {
        let bad = request("zdearo", "has space", RepoVisibility::Private, true);

        assert!(matches!(
            create_remote(Path::new("/nonexistent"), &bad),
            Err(GitHubRemoteError::InvalidRepoName(_))
        ));
    }

    #[test]
    fn create_args_name_the_repo_visibility_source_and_remote() {
        let args = create_args(
            &request("acme", "app", RepoVisibility::Public, false),
            Path::new("/work/app"),
        );

        assert_eq!(
            args,
            [
                "repo",
                "create",
                "acme/app",
                "--public",
                "--source=/work/app",
                "--remote=origin",
            ]
        );
    }

    #[test]
    fn push_is_added_only_when_asked() {
        let args = create_args(
            &request("acme", "app", RepoVisibility::Private, true),
            Path::new("/work/app"),
        );

        assert_eq!(args.last().map(String::as_str), Some("--push"));
        assert!(args.contains(&"--private".to_owned()));
    }

    #[test]
    fn gh_messages_are_named_by_what_they_mean() {
        assert!(matches!(
            classify_failure("To get started with GitHub CLI, please run:  gh auth login"),
            GitHubRemoteError::NotLoggedIn
        ));
        assert!(matches!(
            classify_failure("remote 'origin' already exists"),
            GitHubRemoteError::RemoteExists
        ));
        assert!(matches!(
            classify_failure("name already exists on this account"),
            GitHubRemoteError::NameTaken
        ));
        assert!(matches!(
            classify_failure("boom"),
            GitHubRemoteError::Failed(text) if text == "boom"
        ));
    }
}
