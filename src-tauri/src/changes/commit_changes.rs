//! Commits what is staged, with the summary and description the user wrote.

use std::path::Path;

use crate::error::AppError;
use crate::git::git_command::git;

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

/// Commits the index. Hooks and identity come from the user's git, so their failures are git's
/// own messages.
pub fn commit(root: &Path, summary: &str, description: &str) -> Result<(), AppError> {
    let message = build_message(summary, description)?;
    // git reports "nothing to commit" on stdout, so an empty stderr would reach the user as a blank error.
    if git(root)
        .args(["diff", "--cached", "--name-only"])
        .run()?
        .is_empty()
    {
        return Err(AppError::InvalidCommit("nothing is staged to commit"));
    }
    // `whitespace` keeps lines that start with `#`, which `strip` would delete as comments.
    git(root)
        .args(["commit", "--cleanup=whitespace", "-F", "-"])
        .stdin(message.into_bytes())
        .run()?;
    Ok(())
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

        commit(&repo.path, "Update readme", "# Not a comment\nDetails.").expect("commit");

        let message = repo.git(&["log", "-1", "--format=%B"]);
        assert_eq!(message, "Update readme\n\n# Not a comment\nDetails.");
    }

    #[test]
    fn commit_without_staged_changes_fails_with_git_message() {
        let repo = TestRepository::new();

        let error = commit(&repo.path, "Nothing", "").expect_err("nothing staged");
        assert_eq!(error.to_string(), "nothing is staged to commit");
    }
}
