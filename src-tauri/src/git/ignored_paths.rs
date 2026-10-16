use std::collections::HashSet;
use std::path::Path;

use super::git_command::{GitError, git};

/// Which of `paths` (relative to `root`) Git ignores. A trailing `/` marks a
/// directory, because some patterns only match directories. Tracked files are
/// never reported, even when a pattern would match them.
///
/// A failure means "nothing is known to be ignored": listing files must not
/// depend on this extra decoration.
pub fn ignored_paths(root: &Path, paths: &[String]) -> HashSet<String> {
    if paths.is_empty() {
        return HashSet::new();
    }
    let input = paths
        .iter()
        .flat_map(|path| path.bytes().chain(std::iter::once(0)))
        .collect();

    match git(root)
        .args(["check-ignore", "-z", "--stdin"])
        .stdin(input)
        .run()
    {
        Ok(output) => output
            .split('\0')
            .filter(|path| !path.is_empty())
            .map(str::to_owned)
            .collect(),
        // Exit status 1 is how `check-ignore` says that nothing matched.
        Err(GitError::Failed { stderr, .. }) if stderr.is_empty() => HashSet::new(),
        Err(_) => HashSet::new(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::git::test_repository::TestRepository;

    fn repo_with_gitignore() -> TestRepository {
        let repo = TestRepository::new();
        repo.commit_file(
            ".gitignore",
            "node_modules/\n*.log\ntarget\n",
            "ignore stuff",
        );
        repo
    }

    #[test]
    fn reports_files_and_directories_that_match_the_ignore_rules() {
        let repo = repo_with_gitignore();
        fs::create_dir(repo.path.join("node_modules")).expect("mkdir");

        let ignored = ignored_paths(
            &repo.path,
            &[
                "node_modules/".to_owned(),
                "debug.log".to_owned(),
                "README.md".to_owned(),
                "src/".to_owned(),
            ],
        );

        assert!(ignored.contains("node_modules/"));
        assert!(ignored.contains("debug.log"));
        assert!(!ignored.contains("README.md"));
        assert!(!ignored.contains("src/"));
    }

    #[test]
    fn directory_only_patterns_need_the_trailing_slash() {
        let repo = repo_with_gitignore();
        let without = ignored_paths(&repo.path, &["node_modules".to_owned()]);
        let with = ignored_paths(&repo.path, &["node_modules/".to_owned()]);
        assert!(without.is_empty());
        assert!(with.contains("node_modules/"));
    }

    #[test]
    fn tracked_files_are_not_ignored_even_when_a_rule_matches() {
        let repo = TestRepository::new();
        repo.commit_file("kept.log", "tracked\n", "track a log");
        repo.commit_file(".gitignore", "*.log\n", "ignore logs");

        assert!(ignored_paths(&repo.path, &["kept.log".to_owned()]).is_empty());
    }

    #[test]
    fn nested_gitignore_files_apply_to_their_folder() {
        let repo = TestRepository::new();
        fs::create_dir(repo.path.join("pkg")).expect("mkdir");
        fs::write(repo.path.join("pkg/.gitignore"), "generated.txt\n").expect("write");

        let ignored = ignored_paths(
            &repo.path,
            &["pkg/generated.txt".to_owned(), "generated.txt".to_owned()],
        );

        assert!(ignored.contains("pkg/generated.txt"));
        assert!(!ignored.contains("generated.txt"));
    }

    #[test]
    fn nothing_is_ignored_outside_a_repository_and_for_empty_input() {
        let dir = tempfile::tempdir().expect("temp dir");
        assert!(ignored_paths(dir.path(), &["a.log".to_owned()]).is_empty());
        assert!(ignored_paths(dir.path(), &[]).is_empty());
    }

    #[test]
    fn handles_thousands_of_paths_without_stalling() {
        let repo = repo_with_gitignore();
        let paths: Vec<String> = (0..5000).map(|n| format!("file-{n}.log")).collect();

        let ignored = ignored_paths(&repo.path, &paths);

        assert_eq!(ignored.len(), 5000);
    }
}
