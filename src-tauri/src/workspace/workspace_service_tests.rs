use std::fs;
use std::path::Path;

use tempfile::TempDir;

use crate::error::AppError;
use crate::git::git_command::git;
use crate::git::test_repository::{TestRepository, commit_file_in};
use crate::persistence::AppDatabase;

use super::workspace_models::Repo;
use super::workspace_service::WorkspaceService;

struct Fixture {
    service: WorkspaceService,
    repo: Repo,
    git_repo: TestRepository,
    _state_dir: TempDir,
}

fn service_in(state_dir: &TempDir) -> WorkspaceService {
    let database = AppDatabase::open(&state_dir.path().join("app.db")).expect("open database");
    WorkspaceService::new(database)
}

fn fixture(git_repo: TestRepository) -> Fixture {
    let state_dir = tempfile::tempdir().expect("state dir");
    let service = service_in(&state_dir);
    let repo = service
        .add_repo(&git_repo.path.to_string_lossy())
        .expect("add repo");
    Fixture {
        service,
        repo,
        git_repo,
        _state_dir: state_dir,
    }
}

fn plain_fixture() -> Fixture {
    fixture(TestRepository::new())
}

fn branch_exists(repo: &Path, branch: &str) -> bool {
    git(repo)
        .args(["rev-parse", "--verify", "--quiet"])
        .arg(format!("refs/heads/{branch}"))
        .run()
        .is_ok()
}

#[test]
fn add_repo_registers_the_main_worktree_even_when_given_a_subdirectory() {
    let git_repo = TestRepository::new();
    let subdirectory = git_repo.path.join("src");
    fs::create_dir(&subdirectory).expect("create subdirectory");
    let state_dir = tempfile::tempdir().expect("state dir");
    let service = service_in(&state_dir);

    let repo = service
        .add_repo(&subdirectory.to_string_lossy())
        .expect("add repo");

    assert_eq!(repo.path, git_repo.path.to_string_lossy());
    assert_eq!(repo.name, "repo");
}

#[test]
fn add_repo_resolves_a_linked_worktree_to_its_main_repo() {
    let fixture = plain_fixture();
    let worktree = fixture
        .service
        .create_worktree(&fixture.repo.id, "feature", None)
        .expect("create worktree");

    let again = fixture.service.add_repo(&worktree.path).expect("add again");

    assert_eq!(again, fixture.repo);
    assert_eq!(fixture.service.list_repos().expect("repos").len(), 1);
}

#[test]
fn add_repo_rejects_a_directory_that_is_not_a_repository() {
    let state_dir = tempfile::tempdir().expect("state dir");
    let service = service_in(&state_dir);
    let plain = tempfile::tempdir().expect("plain dir");

    let result = service.add_repo(&plain.path().to_string_lossy());

    assert!(matches!(result, Err(AppError::NotAGitRepo(_))));
    assert!(service.list_repos().expect("repos").is_empty());
}

#[test]
fn add_repo_rejects_a_missing_path() {
    let state_dir = tempfile::tempdir().expect("state dir");
    let service = service_in(&state_dir);

    let result = service.add_repo("/definitely/not/here");

    assert!(matches!(result, Err(AppError::NotAGitRepo(_))));
}

#[test]
fn add_repo_rejects_bare_repositories() {
    let root = tempfile::tempdir().expect("root");
    let bare = root.path().join("bare.git");
    fs::create_dir(&bare).expect("create bare dir");
    git(&bare)
        .args(["init", "--quiet", "--bare"])
        .run()
        .expect("init bare");
    let state_dir = tempfile::tempdir().expect("state dir");
    let service = service_in(&state_dir);

    let result = service.add_repo(&bare.to_string_lossy());

    assert!(matches!(result, Err(AppError::BareRepo)));
}

#[test]
fn repos_survive_reopening_the_store() {
    let git_repo = TestRepository::new();
    let state_dir = tempfile::tempdir().expect("state dir");
    let repo = service_in(&state_dir)
        .add_repo(&git_repo.path.to_string_lossy())
        .expect("add repo");

    let reopened = service_in(&state_dir);

    assert_eq!(reopened.list_repos().expect("repos"), vec![repo]);
}

#[test]
fn create_worktree_makes_the_branch_the_directory_and_remembers_the_base() {
    let fixture = plain_fixture();

    let created = fixture
        .service
        .create_worktree(&fixture.repo.id, "feature/login", Some("main"))
        .expect("create worktree");

    let expected = fixture
        .git_repo
        .path
        .with_file_name("repo-worktrees")
        .join("feature-login");
    assert_eq!(created.path, expected.to_string_lossy());
    assert_eq!(created.name, "feature-login");
    assert_eq!(created.branch.as_deref(), Some("feature/login"));
    assert_eq!(created.base_ref.as_deref(), Some("main"));
    assert!(expected.join("README.md").is_file());

    let listed = fixture
        .service
        .list_worktrees(&fixture.repo.id)
        .expect("list");
    assert_eq!(listed.len(), 2);
    assert!(listed[0].is_main);
    assert_eq!(listed[0].base_ref, None);
    assert_eq!(listed[1].base_ref.as_deref(), Some("main"));
}

#[test]
fn create_worktree_branches_from_the_requested_ref() {
    let fixture = plain_fixture();
    fixture
        .git_repo
        .git(&["checkout", "--quiet", "-b", "release"]);
    fixture
        .git_repo
        .commit_file("release-notes.md", "notes\n", "add notes");
    fixture.git_repo.git(&["checkout", "--quiet", "main"]);

    let created = fixture
        .service
        .create_worktree(&fixture.repo.id, "from-release", Some("release"))
        .expect("create worktree");

    assert!(Path::new(&created.path).join("release-notes.md").is_file());
}

#[test]
fn create_worktree_defaults_to_the_remote_default_branch() {
    let fixture = fixture(TestRepository::new().with_origin());

    let created = fixture
        .service
        .create_worktree(&fixture.repo.id, "task", None)
        .expect("create worktree");

    assert_eq!(created.base_ref.as_deref(), Some("origin/main"));
}

#[test]
fn create_worktree_from_a_remote_ref_does_not_track_it() {
    let fixture = fixture(TestRepository::new().with_origin());

    fixture
        .service
        .create_worktree(&fixture.repo.id, "task", Some("origin/main"))
        .expect("create worktree");

    let tracked = git(&fixture.git_repo.path)
        .args(["config", "--get", "branch.task.remote"])
        .run();
    assert!(tracked.is_err(), "a new branch must not track origin/main");
}

#[test]
fn create_worktree_rejects_invalid_names_and_refs() {
    let fixture = plain_fixture();

    for name in [
        "",
        "   ",
        "has space",
        "-leading-dash",
        "a..b",
        "x@{y}",
        "trailing/",
        "bad~name",
    ] {
        let result = fixture
            .service
            .create_worktree(&fixture.repo.id, name, None);
        assert!(
            matches!(result, Err(AppError::InvalidName(_))),
            "{name:?} should be rejected"
        );
    }

    let result =
        fixture
            .service
            .create_worktree(&fixture.repo.id, "ok", Some("--upload-pack=evil"));
    assert!(matches!(result, Err(AppError::InvalidName(_))));

    assert_eq!(
        fixture
            .service
            .list_worktrees(&fixture.repo.id)
            .expect("list")
            .len(),
        1
    );
}

#[test]
fn create_worktree_rejects_duplicates_without_touching_the_first() {
    let fixture = plain_fixture();
    let first = fixture
        .service
        .create_worktree(&fixture.repo.id, "task", None)
        .expect("first");

    let again = fixture
        .service
        .create_worktree(&fixture.repo.id, "task", None);
    assert!(matches!(again, Err(AppError::AlreadyExists(_))));

    // Different directory, same branch.
    fixture.git_repo.git(&["branch", "taken"]);
    let taken = fixture
        .service
        .create_worktree(&fixture.repo.id, "taken", None);
    assert!(matches!(taken, Err(AppError::AlreadyExists(_))));

    assert!(Path::new(&first.path).is_dir());
    assert_eq!(
        fixture
            .service
            .list_worktrees(&fixture.repo.id)
            .expect("list")
            .len(),
        2
    );
}

#[test]
fn create_worktree_fails_cleanly_for_an_unknown_base() {
    let fixture = plain_fixture();

    let result = fixture
        .service
        .create_worktree(&fixture.repo.id, "task", Some("no-such-ref"));

    assert!(matches!(result, Err(AppError::Git(_))));
    assert!(!branch_exists(&fixture.git_repo.path, "task"));
    assert_eq!(
        fixture
            .service
            .list_worktrees(&fixture.repo.id)
            .expect("list")
            .len(),
        1
    );
}

#[test]
fn unknown_repo_ids_are_reported() {
    let fixture = plain_fixture();

    assert!(matches!(
        fixture.service.list_worktrees("nope"),
        Err(AppError::RepoNotFound(_))
    ));
    assert!(matches!(
        fixture.service.create_worktree("nope", "x", None),
        Err(AppError::RepoNotFound(_))
    ));
}

#[test]
fn list_worktrees_includes_ones_created_outside_the_app() {
    let fixture = plain_fixture();
    let outside = fixture.git_repo.path.with_file_name("outside");
    fixture.git_repo.git(&[
        "worktree",
        "add",
        "--quiet",
        "-b",
        "outside",
        &outside.to_string_lossy(),
    ]);

    let listed = fixture
        .service
        .list_worktrees(&fixture.repo.id)
        .expect("list");

    let found = listed.iter().find(|w| w.name == "outside").expect("listed");
    assert_eq!(found.base_ref, None);
    assert_eq!(found.branch.as_deref(), Some("outside"));
}

#[test]
fn remove_worktree_deletes_the_directory_and_a_merged_branch() {
    let fixture = plain_fixture();
    let created = fixture
        .service
        .create_worktree(&fixture.repo.id, "task", None)
        .expect("create");

    let result = fixture
        .service
        .remove_worktree(&fixture.repo.id, &created.path, false)
        .expect("remove");

    assert!(!Path::new(&created.path).exists());
    assert_eq!(result.branch.as_deref(), Some("task"));
    assert!(result.branch_deleted);
    assert!(!branch_exists(&fixture.git_repo.path, "task"));
    assert_eq!(
        fixture
            .service
            .list_worktrees(&fixture.repo.id)
            .expect("list")
            .len(),
        1
    );
}

#[test]
fn remove_worktree_keeps_a_branch_with_unmerged_commits() {
    let fixture = plain_fixture();
    let created = fixture
        .service
        .create_worktree(&fixture.repo.id, "task", None)
        .expect("create");
    commit_file_in(Path::new(&created.path), "work.txt", "work\n", "do work");

    let result = fixture
        .service
        .remove_worktree(&fixture.repo.id, &created.path, false)
        .expect("remove");

    assert!(!Path::new(&created.path).exists());
    assert!(!result.branch_deleted);
    assert!(result.branch_kept_reason.is_some());
    assert!(
        branch_exists(&fixture.git_repo.path, "task"),
        "the work must stay reachable"
    );
}

#[test]
fn remove_worktree_refuses_uncommitted_changes_unless_forced() {
    let fixture = plain_fixture();
    let created = fixture
        .service
        .create_worktree(&fixture.repo.id, "task", None)
        .expect("create");
    fs::write(Path::new(&created.path).join("scratch.txt"), "wip").expect("write");

    let refused = fixture
        .service
        .remove_worktree(&fixture.repo.id, &created.path, false);
    assert!(matches!(refused, Err(AppError::WorktreeHasChanges)));
    assert!(Path::new(&created.path).join("scratch.txt").is_file());

    fixture
        .service
        .remove_worktree(&fixture.repo.id, &created.path, true)
        .expect("forced remove");
    assert!(!Path::new(&created.path).exists());
}

#[test]
fn remove_worktree_refuses_the_main_worktree_and_unknown_paths() {
    let fixture = plain_fixture();
    let main_path = fixture.git_repo.path.to_string_lossy().into_owned();

    assert!(matches!(
        fixture
            .service
            .remove_worktree(&fixture.repo.id, &main_path, true),
        Err(AppError::CannotRemoveMainWorktree)
    ));
    assert!(matches!(
        fixture
            .service
            .remove_worktree(&fixture.repo.id, "/not/a/worktree", true),
        Err(AppError::WorktreeNotFound(_))
    ));
    assert!(fixture.git_repo.path.join("README.md").is_file());
}

#[test]
fn remove_worktree_handles_a_directory_deleted_behind_our_back() {
    let fixture = plain_fixture();
    let created = fixture
        .service
        .create_worktree(&fixture.repo.id, "task", None)
        .expect("create");
    fs::remove_dir_all(&created.path).expect("delete directory");

    let listed = fixture
        .service
        .list_worktrees(&fixture.repo.id)
        .expect("list");
    assert!(listed.iter().any(|w| w.prunable));

    fixture
        .service
        .remove_worktree(&fixture.repo.id, &created.path, false)
        .expect("remove prunable");
    assert_eq!(
        fixture
            .service
            .list_worktrees(&fixture.repo.id)
            .expect("list")
            .len(),
        1
    );
}

#[test]
fn a_removed_worktree_name_can_be_reused() {
    let fixture = plain_fixture();
    let created = fixture
        .service
        .create_worktree(&fixture.repo.id, "task", None)
        .expect("create");
    fixture
        .service
        .remove_worktree(&fixture.repo.id, &created.path, false)
        .expect("remove");

    let again = fixture
        .service
        .create_worktree(&fixture.repo.id, "task", None)
        .expect("create again");

    assert_eq!(again.path, created.path);
}

#[test]
fn remove_repo_forgets_it_but_leaves_the_disk_alone() {
    let fixture = plain_fixture();
    let created = fixture
        .service
        .create_worktree(&fixture.repo.id, "task", None)
        .expect("create");

    fixture
        .service
        .remove_repo(&fixture.repo.id)
        .expect("remove repo");

    assert!(fixture.service.list_repos().expect("repos").is_empty());
    assert!(Path::new(&created.path).is_dir());
    assert!(matches!(
        fixture.service.remove_repo(&fixture.repo.id),
        Err(AppError::RepoNotFound(_))
    ));

    // Re-adding starts without stale metadata.
    let readded = fixture
        .service
        .add_repo(&fixture.git_repo.path.to_string_lossy())
        .expect("re-add");
    let listed = fixture.service.list_worktrees(&readded.id).expect("list");
    assert!(listed.iter().all(|w| w.base_ref.is_none()));
}

#[test]
fn repo_refs_lists_local_and_remote_branches_without_origin_head() {
    let fixture = fixture(TestRepository::new().with_origin());
    fixture.git_repo.git(&["branch", "topic"]);

    let refs = fixture.service.repo_refs(&fixture.repo.id).expect("refs");

    assert_eq!(refs.default_base, "origin/main");
    assert_eq!(refs.refs, ["main", "origin/main", "topic"]);
}

#[test]
fn repo_refs_falls_back_to_the_current_branch_without_a_remote() {
    let fixture = plain_fixture();

    let refs = fixture.service.repo_refs(&fixture.repo.id).expect("refs");

    assert_eq!(refs.default_base, "main");
}
