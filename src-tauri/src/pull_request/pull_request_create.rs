//! What the "create pull request" form needs, and creating the pull request with `gh pr create`.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::bounded_command::{BoundedError, run_bounded};
use crate::changes::branch_sync;
use crate::error::AppError;
use crate::git::git_command::git;

/// A push and a pull request on a slow connection can take a while.
const CREATE_TIMEOUT: Duration = Duration::from_secs(180);
const MAX_BASES: usize = 100;
const MAX_TITLE_CHARS: usize = 256;
const MAX_BODY_CHARS: usize = 60_000;

#[derive(Debug, thiserror::Error)]
pub enum CreatePullRequestError {
    #[error("the GitHub CLI (gh) is not installed or not on PATH")]
    NotInstalled,
    #[error("the GitHub CLI is not logged in")]
    NotLoggedIn,
    #[error("{0}")]
    Invalid(&'static str),
    #[error("this branch already has a pull request")]
    AlreadyExists,
    #[error("the branch has no commits beyond its base")]
    NoDifference,
    #[error("gh did not answer in time")]
    TimedOut,
    #[error("gh failed: {0}")]
    Failed(String),
}

/// What the form starts from: where the branch can go, and what to call it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestForm {
    pub head_branch: String,
    pub remote: String,
    /// Branches on the remote that can receive the pull request, by short name.
    pub bases: Vec<String>,
    pub default_base: String,
    /// The branch is not on the remote, or has commits it lacks: creating pushes it first.
    pub needs_push: bool,
    pub suggested_title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CreatePullRequestRequest {
    pub base: String,
    pub title: String,
    pub body: String,
    pub draft: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CreatedPullRequest {
    pub url: String,
}

/// Branch names under `refs/remotes/<remote>/`, without the remote prefix and without `HEAD`.
pub fn parse_bases(refs: &str, remote: &str) -> Vec<String> {
    let prefix = format!("{remote}/");
    let mut bases: Vec<String> = refs
        .lines()
        .filter_map(|line| line.trim().strip_prefix(prefix.as_str()))
        .filter(|name| !name.is_empty() && *name != "HEAD")
        .map(str::to_owned)
        .collect();
    bases.sort();
    bases.dedup();
    bases.truncate(MAX_BASES);
    bases
}

/// The remote's default branch when it is listed, else `main` or `master`, else the first one.
pub fn pick_default_base(remote_head: Option<&str>, remote: &str, bases: &[String]) -> String {
    let from_head = remote_head
        .and_then(|head| head.trim().strip_prefix(format!("{remote}/").as_str()))
        .map(str::to_owned);
    from_head
        .filter(|name| bases.contains(name))
        .or_else(|| {
            ["main", "master"]
                .iter()
                .find(|name| bases.iter().any(|b| b == *name))
                .map(|name| (*name).to_owned())
        })
        .or_else(|| bases.first().cloned())
        .unwrap_or_else(|| "main".to_owned())
}

/// A pull request cannot go from a branch into itself: when the default base is the branch being
/// opened, another base is taken if the remote has one.
pub fn avoid_head(default_base: String, head: &str, bases: &[String]) -> String {
    if default_base != head {
        return default_base;
    }
    bases
        .iter()
        .find(|name| name.as_str() != head)
        .cloned()
        .unwrap_or(default_base)
}

/// The one commit's subject when there is exactly one, else the branch name read as words.
pub fn suggest_title(subjects: &[String], branch: &str) -> String {
    if let [only] = subjects {
        return only.clone();
    }
    let words = branch.replace(['-', '_'], " ");
    let mut chars = words.trim().chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// A branch name safe to hand to git and gh: no spaces, no leading hyphen, none of git's special characters.
pub fn is_valid_base(base: &str) -> bool {
    !base.is_empty()
        && base.chars().count() <= 200
        && !base.starts_with('-')
        && !base.contains("..")
        && !base
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || "~^:?*[\\".contains(c))
}

/// The branches a pull request can go into, and the one it goes into by default.
pub struct BaseInfo {
    pub remote: String,
    pub bases: Vec<String>,
    pub default_base: String,
}

/// The remote's branches and its default one, skipping `head_branch` as the default.
pub fn base_info(root: &Path, remote: &str, head_branch: &str) -> Result<BaseInfo, AppError> {
    let refs = git(root)
        .args([
            "for-each-ref",
            "--format=%(refname:short)",
            &format!("refs/remotes/{remote}/"),
        ])
        .run()?;
    let bases = parse_bases(&refs, remote);
    let remote_head = git(root)
        .args([
            "symbolic-ref",
            "--short",
            &format!("refs/remotes/{remote}/HEAD"),
        ])
        .run()
        .ok();
    let default_base = avoid_head(
        pick_default_base(remote_head.as_deref(), remote, &bases),
        head_branch,
        &bases,
    );
    Ok(BaseInfo {
        remote: remote.to_owned(),
        bases,
        default_base,
    })
}

pub fn form(root: &Path) -> Result<PullRequestForm, AppError> {
    let sync = branch_sync::read(root)?;
    let Some(head_branch) = sync.branch.clone() else {
        return Err(AppError::PushUnavailable("the branch is detached"));
    };
    let Some(remote) = sync.remote.clone() else {
        return Err(AppError::PushUnavailable(
            "there is no remote to open a pull request on",
        ));
    };
    let BaseInfo {
        remote,
        bases,
        default_base,
    } = base_info(root, &remote, &head_branch)?;

    // The subjects ahead of the base are only a hint for the title: without the base ref there are none.
    let subjects: Vec<String> = git(root)
        .args([
            "log",
            "--format=%s",
            "-n",
            "20",
            &format!("{remote}/{default_base}..HEAD"),
        ])
        .run()
        .map(|text| text.lines().map(str::to_owned).collect())
        .unwrap_or_default();

    Ok(PullRequestForm {
        suggested_title: suggest_title(&subjects, &head_branch),
        needs_push: sync.upstream.is_none() || sync.ahead > 0,
        head_branch,
        remote,
        bases,
        default_base,
    })
}

pub fn validate(request: &CreatePullRequestRequest) -> Result<(), CreatePullRequestError> {
    let title = request.title.trim();
    if title.is_empty() {
        return Err(CreatePullRequestError::Invalid(
            "the pull request needs a title",
        ));
    }
    if title.contains(['\n', '\r', '\0']) || title.chars().count() > MAX_TITLE_CHARS {
        return Err(CreatePullRequestError::Invalid(
            "the title must be one short line",
        ));
    }
    if request.body.contains('\0') || request.body.chars().count() > MAX_BODY_CHARS {
        return Err(CreatePullRequestError::Invalid(
            "the description is too long",
        ));
    }
    if !is_valid_base(&request.base) {
        return Err(CreatePullRequestError::Invalid(
            "the base branch name is not valid",
        ));
    }
    Ok(())
}

/// The arguments for `gh pr create`. The body goes on stdin, and values use `--flag=value` so a
/// title that starts with a hyphen is never read as a flag.
pub fn create_args(request: &CreatePullRequestRequest) -> Vec<String> {
    let mut args = vec![
        "pr".to_owned(),
        "create".to_owned(),
        format!("--base={}", request.base),
        format!("--title={}", request.title.trim()),
        "--body-file=-".to_owned(),
    ];
    if request.draft {
        args.push("--draft".to_owned());
    }
    args
}

pub fn classify_failure(stderr: &str) -> CreatePullRequestError {
    let text = stderr.to_lowercase();
    if text.contains("gh auth login") || text.contains("not logged in") {
        return CreatePullRequestError::NotLoggedIn;
    }
    if text.contains("already exists") {
        return CreatePullRequestError::AlreadyExists;
    }
    if text.contains("no commits between") {
        return CreatePullRequestError::NoDifference;
    }
    CreatePullRequestError::Failed(stderr.trim().to_owned())
}

/// Pushes the branch when the remote lacks it, then opens the pull request from it.
pub fn create(
    root: &Path,
    request: &CreatePullRequestRequest,
) -> Result<CreatedPullRequest, AppError> {
    validate(request)?;
    let sync = branch_sync::read(root)?;
    if sync.upstream.is_none() || sync.ahead > 0 {
        branch_sync::push(root)?;
    }

    let mut command = Command::new("gh");
    command
        .args(create_args(request))
        .current_dir(root)
        .env("GH_PROMPT_DISABLED", "1");
    let output =
        run_bounded(&mut command, request.body.as_bytes(), CREATE_TIMEOUT).map_err(|error| {
            match error {
                BoundedError::TimedOut => CreatePullRequestError::TimedOut,
                BoundedError::Spawn(source) if source.kind() == std::io::ErrorKind::NotFound => {
                    CreatePullRequestError::NotInstalled
                }
                BoundedError::Spawn(source) => CreatePullRequestError::Failed(source.to_string()),
            }
        })?;
    if !output.status.success() {
        return Err(classify_failure(&String::from_utf8_lossy(&output.stderr)).into());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let url = stdout
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| line.starts_with("https://"))
        .ok_or_else(|| {
            CreatePullRequestError::Failed("gh did not print the pull request link".to_owned())
        })?;
    Ok(CreatedPullRequest {
        url: url.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(title: &str) -> CreatePullRequestRequest {
        CreatePullRequestRequest {
            base: "main".to_owned(),
            title: title.to_owned(),
            body: "Body.".to_owned(),
            draft: false,
        }
    }

    #[test]
    fn bases_are_the_remote_branches_without_prefix_or_head() {
        let refs = "origin\norigin/HEAD\norigin/main\norigin/feat/x\norigin/main\n";

        assert_eq!(parse_bases(refs, "origin"), ["feat/x", "main"]);
    }

    #[test]
    fn the_default_base_follows_the_remote_head_then_main_then_master() {
        let bases = vec!["develop".to_owned(), "main".to_owned()];
        assert_eq!(
            pick_default_base(Some("origin/develop"), "origin", &bases),
            "develop"
        );
        assert_eq!(pick_default_base(None, "origin", &bases), "main");
        assert_eq!(
            pick_default_base(None, "origin", &["master".to_owned()]),
            "master"
        );
        assert_eq!(
            pick_default_base(None, "origin", &["zeta".to_owned()]),
            "zeta"
        );
        assert_eq!(pick_default_base(None, "origin", &[]), "main");
    }

    #[test]
    fn a_branch_is_never_its_own_default_base_when_the_remote_has_another() {
        let bases = vec!["develop".to_owned(), "main".to_owned()];

        assert_eq!(avoid_head("main".to_owned(), "main", &bases), "develop");
        assert_eq!(avoid_head("main".to_owned(), "feat/x", &bases), "main");
        // With only the one branch there is nothing else to offer.
        assert_eq!(
            avoid_head("main".to_owned(), "main", &["main".to_owned()]),
            "main"
        );
    }

    #[test]
    fn the_title_is_the_only_commit_or_the_branch_as_words() {
        assert_eq!(
            suggest_title(&["fix: refresh the token".to_owned()], "feat/auth"),
            "fix: refresh the token"
        );
        assert_eq!(
            suggest_title(&[], "refactor-token_cache"),
            "Refactor token cache"
        );
        assert_eq!(
            suggest_title(&["a".to_owned(), "b".to_owned()], "feat/auth"),
            "Feat/auth"
        );
    }

    #[test]
    fn base_names_cannot_smuggle_flags_or_odd_characters() {
        assert!(is_valid_base("main"));
        assert!(is_valid_base("release/1.2"));
        assert!(!is_valid_base(""));
        assert!(!is_valid_base("--force"));
        assert!(!is_valid_base("a b"));
        assert!(!is_valid_base("a..b"));
        assert!(!is_valid_base("a:b"));
    }

    #[test]
    fn a_pull_request_needs_a_one_line_title_and_a_sane_body() {
        assert!(validate(&request("Fix the label")).is_ok());
        assert!(validate(&request("   ")).is_err());
        assert!(validate(&request("two\nlines")).is_err());
        let mut long_body = request("ok");
        long_body.body = "x".repeat(MAX_BODY_CHARS + 1);
        assert!(validate(&long_body).is_err());
    }

    #[test]
    fn the_arguments_put_values_after_equals_and_add_draft_only_when_asked() {
        let mut draft = request("-odd title");
        draft.draft = true;

        let args = create_args(&draft);

        assert!(args.contains(&"--title=-odd title".to_owned()));
        assert!(args.contains(&"--base=main".to_owned()));
        assert_eq!(args.last().map(String::as_str), Some("--draft"));
        assert!(!create_args(&request("t")).contains(&"--draft".to_owned()));
    }

    #[test]
    fn gh_messages_are_named_by_what_they_mean() {
        assert!(matches!(
            classify_failure("a pull request for branch \"x\" into branch \"main\" already exists"),
            CreatePullRequestError::AlreadyExists
        ));
        assert!(matches!(
            classify_failure("pull request create failed: No commits between main and x"),
            CreatePullRequestError::NoDifference
        ));
        assert!(matches!(
            classify_failure("run: gh auth login"),
            CreatePullRequestError::NotLoggedIn
        ));
    }

    #[test]
    fn the_form_reads_the_bases_the_default_and_whether_a_push_is_needed() {
        use crate::git::test_repository::TestRepository;
        let repo = TestRepository::new().with_origin();
        repo.git(&["checkout", "-q", "-b", "feat/new-thing"]);
        repo.commit_file("a.txt", "a\n", "add a");

        let form = form(&repo.path).expect("form");

        assert_eq!(form.head_branch, "feat/new-thing");
        assert_eq!(form.remote, "origin");
        assert_eq!(form.bases, ["main"]);
        assert_eq!(form.default_base, "main");
        assert!(form.needs_push);
        assert_eq!(form.suggested_title, "add a");
    }
}
