//! Commits what is staged, with the summary and description the user wrote.

use std::path::Path;

use crate::error::AppError;
use crate::git::git_command::git;

use super::branch_sync;

/// Generous for a summary; git's own convention is about 72 columns, but nothing here wraps it.
const MAX_SUMMARY_CHARS: usize = 200;
const MAX_DESCRIPTION_CHARS: usize = 10_000;

/// Git's message: the summary, a blank line, then the description. Refuses what git would
/// store badly: a summary that spans lines, or NUL bytes.
pub fn build_message(summary: &str, description: &str) -> Result<String, AppError> {
    let summary = summary.trim();
    if summary.is_empty() {
        return Err(AppError::InvalidCommit("the commit needs a summary"));
    }
    if summary.contains(['\n', '\r']) {
        return Err(AppError::InvalidCommit("the summary must be one line"));
    }
    if summary.chars().count() > MAX_SUMMARY_CHARS {
        return Err(AppError::InvalidCommit("the summary is too long"));
    }
    if description.chars().count() > MAX_DESCRIPTION_CHARS {
        return Err(AppError::InvalidCommit("the description is too long"));
    }
    if summary.contains('\0') || description.contains('\0') {
        return Err(AppError::InvalidCommit(
            "the message contains a NUL character",
        ));
    }

    let description = description.trim();
    if description.is_empty() {
        Ok(format!("{summary}\n"))
    } else {
        Ok(format!("{summary}\n\n{description}\n"))
    }
}

/// Whether the last commit is already on the branch's upstream, so rewriting it would diverge.
fn head_is_pushed(root: &Path) -> Result<bool, AppError> {
    let sync = branch_sync::read(root)?;
    Ok(sync.upstream.is_some() && sync.ahead == 0)
}

/// How a commit is made: a new one, or a rewrite of the last one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitMode {
    New,
    /// Replaces the last commit. `allow_pushed` is the user's yes when it is already on the remote.
    Amend {
        allow_pushed: bool,
    },
}

/// Commits the index. Hooks and identity come from the user's git, so their failures are git's
/// own messages. An amend may have nothing staged: it can change only the message.
pub fn commit(
    root: &Path,
    summary: &str,
    description: &str,
    mode: CommitMode,
) -> Result<(), AppError> {
    let message = build_message(summary, description)?;
    let mut args = vec!["commit", "--cleanup=whitespace", "-F", "-"];
    match mode {
        CommitMode::New => {
            // git reports "nothing to commit" on stdout, so an empty stderr would reach the user as a blank error.
            if git(root)
                .args(["diff", "--cached", "--name-only"])
                .run()?
                .is_empty()
            {
                return Err(AppError::InvalidCommit("nothing is staged to commit"));
            }
        }
        CommitMode::Amend { allow_pushed } => {
            if git(root)
                .args(["rev-parse", "--verify", "-q", "HEAD"])
                .run()
                .is_err()
            {
                return Err(AppError::ChangeRefused("there is no commit to amend yet"));
            }
            if !allow_pushed && head_is_pushed(root)? {
                return Err(AppError::ChangeRefused(
                    "the last commit is already on the remote; amending it needs a force push",
                ));
            }
            args.push("--amend");
        }
    }
    // `whitespace` keeps lines that start with `#`, which `strip` would delete as comments.
    git(root).args(args).stdin(message.into_bytes()).run()?;
    Ok(())
}

/// The summary and description of the last commit, to start an amend from.
pub fn last_message(root: &Path) -> Result<(String, String), AppError> {
    let text = git(root).args(["log", "-1", "--format=%B"]).run()?;
    let (summary, rest) = text.split_once('\n').unwrap_or((text.as_str(), ""));
    Ok((summary.trim().to_owned(), rest.trim().to_owned()))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::changes::change_staging::stage;
    use crate::git::test_repository::TestRepository;

    #[test]
    fn summary_alone_is_the_whole_message() {
        assert_eq!(build_message("  Fix login  ", "  ").unwrap(), "Fix login\n");
    }

    #[test]
    fn description_follows_a_blank_line() {
        assert_eq!(
            build_message("Fix login", "Because the token expired.\n\n").unwrap(),
            "Fix login\n\nBecause the token expired.\n"
        );
    }

    #[test]
    fn a_summary_is_required_and_has_one_line() {
        assert!(build_message("   ", "body").is_err());
        assert!(build_message("two\nlines", "").is_err());
    }

    #[test]
    fn limits_and_nul_bytes_are_refused() {
        assert!(build_message(&"a".repeat(MAX_SUMMARY_CHARS + 1), "").is_err());
        assert!(build_message("ok", &"b".repeat(MAX_DESCRIPTION_CHARS + 1)).is_err());
        assert!(build_message("with\0nul", "").is_err());
    }

    #[test]
    fn commit_stores_the_message_and_keeps_hash_lines() {
        let repo = TestRepository::new();
        fs::write(repo.path.join("README.md"), "changed\n").expect("write");
        stage(&repo.path, "README.md").expect("stage");

        commit(
            &repo.path,
            "Update readme",
            "# Not a comment\nDetails.",
            CommitMode::New,
        )
        .expect("commit");

        let message = repo.git(&["log", "-1", "--format=%B"]);
        assert_eq!(message, "Update readme\n\n# Not a comment\nDetails.");
    }

    #[test]
    fn commit_without_staged_changes_fails_with_git_message() {
        let repo = TestRepository::new();

        let error = commit(&repo.path, "Nothing", "", CommitMode::New).expect_err("nothing staged");
        assert_eq!(error.to_string(), "nothing is staged to commit");
    }

    #[test]
    fn amending_rewrites_the_last_commit_with_a_new_message_and_nothing_staged() {
        let repo = TestRepository::new();
        let before = repo.git(&["rev-list", "--count", "HEAD"]);

        commit(
            &repo.path,
            "Better words",
            "Body.",
            CommitMode::Amend {
                allow_pushed: false,
            },
        )
        .expect("amend");

        assert_eq!(repo.git(&["rev-list", "--count", "HEAD"]), before);
        assert_eq!(
            last_message(&repo.path).expect("message"),
            ("Better words".to_owned(), "Body.".to_owned())
        );
    }

    #[test]
    fn amending_folds_staged_files_into_the_last_commit() {
        let repo = TestRepository::new();
        fs::write(repo.path.join("extra.txt"), "x\n").expect("write");
        stage(&repo.path, "extra.txt").expect("stage");

        commit(
            &repo.path,
            "initial commit",
            "",
            CommitMode::Amend {
                allow_pushed: false,
            },
        )
        .expect("amend");

        assert!(
            repo.git(&["show", "--name-only", "--format=", "HEAD"])
                .contains("extra.txt")
        );
    }

    #[test]
    fn a_pushed_commit_is_only_amended_with_the_users_yes() {
        let repo = TestRepository::new().with_origin();
        crate::changes::branch_sync::push(&repo.path).expect("publish");

        let refused = commit(
            &repo.path,
            "Rewrite",
            "",
            CommitMode::Amend {
                allow_pushed: false,
            },
        )
        .expect_err("pushed");
        assert!(matches!(refused, AppError::ChangeRefused(_)));

        commit(
            &repo.path,
            "Rewrite",
            "",
            CommitMode::Amend { allow_pushed: true },
        )
        .expect("allowed");
        assert_eq!(last_message(&repo.path).expect("message").0, "Rewrite");
    }
}
