//! Throwaway repositories for tests. Everything lives in one temp directory.

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::git_command::git;

pub struct TestRepository {
    // Held so the directory is removed when the repository goes out of scope.
    _root: TempDir,
    pub path: PathBuf,
}

impl TestRepository {
    /// A repository on `main` with one commit.
    pub fn new() -> Self {
        let root = tempfile::tempdir().expect("temp dir");
        // Canonical because Git reports real paths (macOS temp dirs are symlinks).
        let path = fs::canonicalize(root.path())
            .expect("canonical temp dir")
            .join("repo");
        fs::create_dir(&path).expect("create repo dir");

        let repository = Self { _root: root, path };
        repository.git(&["init", "--quiet"]);
        repository.git(&["symbolic-ref", "HEAD", "refs/heads/main"]);
        repository.commit_file("README.md", "hello\n", "initial commit");
        repository
    }

    pub fn git(&self, args: &[&str]) -> String {
        run(&self.path, args)
    }

    pub fn commit_file(&self, name: &str, content: &str, message: &str) {
        commit_file_in(&self.path, name, content, message);
    }

    /// Adds a bare `origin` next to the repository, pushes `main` and points
    /// `origin/HEAD` at it, like a fresh clone would have.
    pub fn with_origin(self) -> Self {
        let origin = self.path.with_file_name("origin.git");
        fs::create_dir(&origin).expect("create origin dir");
        run(&origin, &["init", "--quiet", "--bare"]);
        self.git(&["remote", "add", "origin", &origin.to_string_lossy()]);
        self.git(&["push", "--quiet", "origin", "main"]);
        self.git(&["fetch", "--quiet", "origin"]);
        self.git(&["remote", "set-head", "origin", "main"]);
        self
    }
}

pub fn commit_file_in(dir: &Path, name: &str, content: &str, message: &str) {
    fs::write(dir.join(name), content).expect("write file");
    run(dir, &["add", name]);
    run(
        dir,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "-m",
            message,
        ],
    );
}

fn run(dir: &Path, args: &[&str]) -> String {
    git(dir)
        .args(args)
        .run()
        .unwrap_or_else(|error| panic!("test git command failed: {error}"))
}
