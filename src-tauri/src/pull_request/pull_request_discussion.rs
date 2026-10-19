//! What people said on a pull request: the conversation, the reviews and the inline threads, and
//! the ways to answer. All through the user's `gh`.

use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use specta::Type;

use super::github_remote::{GitHubRemoteError, run_gh, run_gh_with_input};

const TIMEOUT: Duration = Duration::from_secs(60);
const MAX_BODY_CHARS: usize = 60_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DiscussionComment {
    pub id: String,
    pub author: String,
    pub body: String,
    /// When it was written, as GitHub prints it (ISO 8601).
    pub created_at: String,
    pub url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ReviewVerdict {
    Approved,
    ChangesRequested,
    Commented,
    Dismissed,
    Pending,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReviewSummary {
    pub author: String,
    pub verdict: ReviewVerdict,
    pub body: String,
    pub submitted_at: String,
}

/// A conversation on a line of the diff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReviewThread {
    pub id: String,
    pub path: String,
    pub line: Option<u32>,
    pub resolved: bool,
    /// The code it was about has changed since.
    pub outdated: bool,
    pub comments: Vec<DiscussionComment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Discussion {
    pub comments: Vec<DiscussionComment>,
    pub reviews: Vec<ReviewSummary>,
    pub threads: Vec<ReviewThread>,
}

#[derive(Deserialize)]
struct RawAuthor {
    login: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawComment {
    id: Option<String>,
    author: Option<RawAuthor>,
    #[serde(default)]
    body: String,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    url: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawReview {
    author: Option<RawAuthor>,
    #[serde(default)]
    body: String,
    #[serde(default)]
    state: String,
    #[serde(default)]
    submitted_at: Option<String>,
}

#[derive(Deserialize)]
struct RawView {
    #[serde(default)]
    comments: Vec<RawComment>,
    #[serde(default)]
    reviews: Vec<RawReview>,
}

fn comment_of(raw: RawComment) -> DiscussionComment {
    DiscussionComment {
        id: raw.id.unwrap_or_default(),
        author: raw
            .author
            .and_then(|author| author.login)
            .unwrap_or_else(|| "ghost".to_owned()),
        body: raw.body,
        created_at: raw.created_at,
        url: raw.url,
    }
}

fn verdict_of(state: &str) -> ReviewVerdict {
    match state {
        "APPROVED" => ReviewVerdict::Approved,
        "CHANGES_REQUESTED" => ReviewVerdict::ChangesRequested,
        "DISMISSED" => ReviewVerdict::Dismissed,
        "PENDING" => ReviewVerdict::Pending,
        _ => ReviewVerdict::Commented,
    }
}

/// Reads `gh pr view --json comments,reviews`. A review with no words and a plain "commented"
/// state only wraps inline comments, which the threads already show, so it is left out.
pub fn parse_view(
    json: &str,
) -> Result<(Vec<DiscussionComment>, Vec<ReviewSummary>), GitHubRemoteError> {
    let view: RawView = serde_json::from_str(json).map_err(|_| unreadable())?;
    let comments = view.comments.into_iter().map(comment_of).collect();
    let reviews = view
        .reviews
        .into_iter()
        .filter(|review| !(review.body.trim().is_empty() && review.state == "COMMENTED"))
        .map(|review| ReviewSummary {
            author: review
                .author
                .and_then(|author| author.login)
                .unwrap_or_else(|| "ghost".to_owned()),
            verdict: verdict_of(&review.state),
            body: review.body,
            submitted_at: review.submitted_at.unwrap_or_default(),
        })
        .collect();
    Ok((comments, reviews))
}

fn unreadable() -> GitHubRemoteError {
    GitHubRemoteError::Failed("gh answered with something Lynx cannot read".to_owned())
}

#[derive(Deserialize)]
struct RawThreads {
    data: RawData,
}
#[derive(Deserialize)]
struct RawData {
    repository: RawRepository,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawRepository {
    pull_request: RawPullRequest,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPullRequest {
    review_threads: RawThreadList,
}
#[derive(Deserialize)]
struct RawThreadList {
    nodes: Vec<RawThread>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawThread {
    id: String,
    #[serde(default)]
    is_resolved: bool,
    #[serde(default)]
    is_outdated: bool,
    #[serde(default)]
    path: String,
    line: Option<u32>,
    comments: RawCommentList,
}
#[derive(Deserialize)]
struct RawCommentList {
    nodes: Vec<RawComment>,
}

/// Reads the GraphQL answer for the review threads.
pub fn parse_threads(json: &str) -> Result<Vec<ReviewThread>, GitHubRemoteError> {
    let raw: RawThreads = serde_json::from_str(json).map_err(|_| unreadable())?;
    Ok(raw
        .data
        .repository
        .pull_request
        .review_threads
        .nodes
        .into_iter()
        .map(|thread| ReviewThread {
            id: thread.id,
            path: thread.path,
            line: thread.line,
            resolved: thread.is_resolved,
            outdated: thread.is_outdated,
            comments: thread.comments.nodes.into_iter().map(comment_of).collect(),
        })
        .collect())
}

const THREADS_QUERY: &str = "query($owner:String!,$repo:String!,$number:Int!){repository(owner:$owner,name:$repo){pullRequest(number:$number){reviewThreads(first:50){nodes{id isResolved isOutdated path line comments(first:50){nodes{id author{login} body createdAt url}}}}}}}";
const REPLY_MUTATION: &str = "mutation($thread:ID!,$body:String!){addPullRequestReviewThreadReply(input:{pullRequestReviewThreadId:$thread,body:$body}){comment{id}}}";
const RESOLVE_MUTATION: &str =
    "mutation($thread:ID!){resolveReviewThread(input:{threadId:$thread}){thread{id}}}";
const UNRESOLVE_MUTATION: &str =
    "mutation($thread:ID!){unresolveReviewThread(input:{threadId:$thread}){thread{id}}}";

/// The conversation, the reviews and the inline threads of one pull request.
pub fn read(root: &Path, number: u32) -> Result<Discussion, GitHubRemoteError> {
    let number_text = number.to_string();
    let view = run_gh(
        &["pr", "view", &number_text, "--json", "comments,reviews"],
        Some(root),
        TIMEOUT,
    )?;
    let (comments, reviews) = parse_view(&view)?;
    // `{owner}` and `{repo}` are filled in by gh from the repository the folder belongs to.
    let threads = run_gh(
        &[
            "api",
            "graphql",
            "-f",
            &format!("query={THREADS_QUERY}"),
            "-F",
            "owner={owner}",
            "-F",
            "repo={repo}",
            "-F",
            &format!("number={number}"),
        ],
        Some(root),
        TIMEOUT,
    )
    .and_then(|json| parse_threads(&json))
    // Inline threads are the extra: without them the conversation still reads.
    .unwrap_or_default();
    Ok(Discussion {
        comments,
        reviews,
        threads,
    })
}

fn check_body(body: &str) -> Result<(), GitHubRemoteError> {
    if body.trim().is_empty() {
        return Err(GitHubRemoteError::Failed(
            "write something first".to_owned(),
        ));
    }
    if body.chars().count() > MAX_BODY_CHARS || body.contains('\0') {
        return Err(GitHubRemoteError::Failed(
            "the comment is too long".to_owned(),
        ));
    }
    Ok(())
}

/// A thread id is GitHub's opaque node id: letters, digits, `_` and `-`. Nothing else reaches gh.
pub fn is_node_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 100
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
}

/// A comment on the conversation of the pull request. The text goes on stdin.
pub fn add_comment(root: &Path, number: u32, body: &str) -> Result<(), GitHubRemoteError> {
    check_body(body)?;
    run_gh_with_input(
        &["pr", "comment", &number.to_string(), "--body-file", "-"],
        Some(root),
        body.as_bytes(),
        TIMEOUT,
    )?;
    Ok(())
}

pub fn reply_to_thread(root: &Path, thread_id: &str, body: &str) -> Result<(), GitHubRemoteError> {
    check_body(body)?;
    if !is_node_id(thread_id) {
        return Err(GitHubRemoteError::Failed(
            "that is not a thread id".to_owned(),
        ));
    }
    run_gh(
        &[
            "api",
            "graphql",
            "-f",
            &format!("query={REPLY_MUTATION}"),
            "-F",
            &format!("thread={thread_id}"),
            "-f",
            &format!("body={body}"),
        ],
        Some(root),
        TIMEOUT,
    )?;
    Ok(())
}

pub fn set_thread_resolved(
    root: &Path,
    thread_id: &str,
    resolved: bool,
) -> Result<(), GitHubRemoteError> {
    if !is_node_id(thread_id) {
        return Err(GitHubRemoteError::Failed(
            "that is not a thread id".to_owned(),
        ));
    }
    let mutation = if resolved {
        RESOLVE_MUTATION
    } else {
        UNRESOLVE_MUTATION
    };
    run_gh(
        &[
            "api",
            "graphql",
            "-f",
            &format!("query={mutation}"),
            "-F",
            &format!("thread={thread_id}"),
        ],
        Some(root),
        TIMEOUT,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_view_gives_comments_and_reviews_and_skips_empty_comment_wrappers() {
        let json = r#"{
            "comments": [{"id": "IC_1", "author": {"login": "ana"}, "body": "Looks good",
                          "createdAt": "2026-10-19T10:00:00Z", "url": "https://github.com/a/b/pull/1#c1"}],
            "reviews": [
                {"author": {"login": "bruno"}, "body": "", "state": "COMMENTED", "submittedAt": "2026-10-19T11:00:00Z"},
                {"author": {"login": "carla"}, "body": "Needs a test", "state": "CHANGES_REQUESTED", "submittedAt": "2026-10-19T12:00:00Z"},
                {"author": null, "body": "", "state": "APPROVED", "submittedAt": "2026-10-19T13:00:00Z"}
            ]
        }"#;

        let (comments, reviews) = parse_view(json).expect("view");

        assert_eq!(comments[0].author, "ana");
        assert_eq!(reviews.len(), 2);
        assert_eq!(reviews[0].verdict, ReviewVerdict::ChangesRequested);
        assert_eq!(reviews[1].author, "ghost");
        assert_eq!(reviews[1].verdict, ReviewVerdict::Approved);
    }

    #[test]
    fn threads_carry_their_path_line_state_and_comments() {
        let json = r#"{"data": {"repository": {"pullRequest": {"reviewThreads": {"nodes": [
            {"id": "PRRT_abc", "isResolved": false, "isOutdated": true, "path": "src/a.rs", "line": 12,
             "comments": {"nodes": [{"id": "C1", "author": {"login": "ana"}, "body": "Why?",
                                     "createdAt": "2026-10-19T10:00:00Z", "url": "u"}]}}
        ]}}}}}"#;

        let threads = parse_threads(json).expect("threads");

        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].path, "src/a.rs");
        assert_eq!(threads[0].line, Some(12));
        assert!(threads[0].outdated && !threads[0].resolved);
        assert_eq!(threads[0].comments[0].body, "Why?");
        assert!(parse_threads("nope").is_err());
    }

    #[test]
    fn only_node_ids_and_real_text_reach_gh() {
        assert!(is_node_id("PRRT_kwDOA-bc123"));
        assert!(!is_node_id(""));
        assert!(!is_node_id("PRRT abc"));
        assert!(!is_node_id("x\"}) { y"));
        assert!(check_body("hello").is_ok());
        assert!(check_body("   ").is_err());
        assert!(check_body(&"a".repeat(MAX_BODY_CHARS + 1)).is_err());
    }
}
