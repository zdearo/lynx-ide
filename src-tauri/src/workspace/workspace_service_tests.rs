use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

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
    service_with_root(state_dir, None)
}

fn service_with_root(state_dir: &TempDir, root: Option<PathBuf>) -> WorkspaceService {
    let database = AppDatabase::open(&state_dir.path().join("app.db")).expect("open database");
    WorkspaceService::new(database, Arc::new(move || root.clone()))
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
fn create_project_makes_an_empty_repo_and_registers_it() {
    let state_dir = tempfile::tempdir().expect("state dir");
    let service = service_in(&state_dir);
    let parent = tempfile::tempdir().expect("parent");

    let repo = service
        .create_project(&parent.path().to_string_lossy(), "my-app")
        .expect("create project");

    assert_eq!(repo.name, "my-app");
    assert!(parent.path().join("my-app").join(".git").is_dir());
    assert_eq!(service.list_repos().expect("repos").len(), 1);
    let worktrees = service.list_worktrees(&repo.id).expect("worktrees");
    assert_eq!(worktrees.len(), 1);
    assert!(worktrees[0].is_main);
}

#[test]
fn create_project_registers_nothing_when_the_name_is_refused() {
    let state_dir = tempfile::tempdir().expect("state dir");
    let service = service_in(&state_dir);
    let parent = tempfile::tempdir().expect("parent");

    let result = service.create_project(&parent.path().to_string_lossy(), "a/b");

    assert!(matches!(result, Err(AppError::InvalidName(_))));
    assert!(service.list_repos().expect("repos").is_empty());
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

/// A worktree on `task` with one commit of work, and that same work squashed into the main branch.
fn squash_merged_fixture() -> (Fixture, String, String) {
    let fixture = plain_fixture();
    let created = fixture
        .service
        .create_worktree(&fixture.repo.id, "task", None)
        .expect("create");
    commit_file_in(Path::new(&created.path), "work.txt", "work\n", "do work");
    let tip = fixture.git_repo.git(&["rev-parse", "task"]);

    // What a squash merge on GitHub leaves behind: the work on main, as a new commit.
    fixture.git_repo.git(&["merge", "--squash", "task"]);
    fixture.git_repo.git(&["commit", "-m", "task (#1)"]);
    (fixture, created.path, tip)
}

#[test]
fn a_squash_merged_branch_is_deleted_when_it_still_points_at_the_merged_commit() {
    let (fixture, path, tip) = squash_merged_fixture();

    // `branch -d` alone would keep it: its commit is not an ancestor of main.
    let result = fixture
        .service
        .remove_merged_worktree(&fixture.repo.id, &path, &tip)
        .expect("clean up");

    assert!(!Path::new(&path).exists());
    assert!(result.branch_deleted);
    assert_eq!(result.branch_kept_reason, None);
    assert!(!branch_exists(&fixture.git_repo.path, "task"));
}

#[test]
fn a_plain_removal_keeps_a_squash_merged_branch_which_is_why_the_clean_up_exists() {
    let (fixture, path, _) = squash_merged_fixture();

    let result = fixture
        .service
        .remove_worktree(&fixture.repo.id, &path, false)
        .expect("remove");

    assert!(!result.branch_deleted);
    assert!(branch_exists(&fixture.git_repo.path, "task"));
}

#[test]
fn a_branch_that_moved_on_after_the_merge_is_kept() {
    let (fixture, path, merged_tip) = squash_merged_fixture();
    commit_file_in(
        Path::new(&path),
        "later.txt",
        "later\n",
        "work after the merge",
    );

    let result = fixture
        .service
        .remove_merged_worktree(&fixture.repo.id, &path, &merged_tip)
        .expect("clean up");

    assert!(!Path::new(&path).exists());
    assert!(!result.branch_deleted);
    assert!(result.branch_kept_reason.is_some());
    assert!(
        branch_exists(&fixture.git_repo.path, "task"),
        "the later commit exists nowhere else"
    );
}

#[test]
fn a_merged_clean_up_never_discards_uncommitted_changes() {
    let (fixture, path, tip) = squash_merged_fixture();
    fs::write(Path::new(&path).join("work.txt"), "edited since\n").expect("write");

    let error = fixture
        .service
        .remove_merged_worktree(&fixture.repo.id, &path, &tip)
        .expect_err("uncommitted changes");

    assert!(matches!(error, AppError::WorktreeHasChanges));
    assert!(Path::new(&path).exists());
    assert!(branch_exists(&fixture.git_repo.path, "task"));
}

#[test]
fn a_merged_clean_up_refuses_the_main_worktree() {
    let fixture = plain_fixture();
    let main_path = fixture.git_repo.path.to_string_lossy().into_owned();

    let error = fixture
        .service
        .remove_merged_worktree(&fixture.repo.id, &main_path, "deadbeef")
        .expect_err("main worktree");

    assert!(matches!(error, AppError::CannotRemoveMainWorktree));
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

#[test]
fn a_configured_root_holds_worktrees_grouped_by_repository() {
    let git_repo = TestRepository::new();
    let state_dir = tempfile::tempdir().expect("state dir");
    let root = tempfile::tempdir().expect("root dir");
    let root_path = fs::canonicalize(root.path()).expect("canonical");
    let service = service_with_root(&state_dir, Some(root_path.clone()));
    let repo = service
        .add_repo(&git_repo.path.to_string_lossy())
        .expect("add repo");

    let created = service
        .create_worktree(&repo.id, "feature/login", None)
        .expect("create worktree");

    assert_eq!(
        created.path,
        root_path
            .join("repo")
            .join("feature-login")
            .to_string_lossy()
    );
    assert!(Path::new(&created.path).join("README.md").is_file());
    assert!(
        !git_repo.path.with_file_name("repo-worktrees").exists(),
        "nothing is created next to the repository"
    );
}

#[test]
fn worktrees_made_before_the_setting_changed_stay_where_they_are() {
    let git_repo = TestRepository::new();
    let state_dir = tempfile::tempdir().expect("state dir");
    let before = service_in(&state_dir);
    let repo = before
        .add_repo(&git_repo.path.to_string_lossy())
        .expect("add repo");
    let old = before
        .create_worktree(&repo.id, "old", None)
        .expect("create old");

    let root = tempfile::tempdir().expect("root dir");
    let after = service_with_root(&state_dir, Some(root.path().to_owned()));
    let listed = after.list_worktrees(&repo.id).expect("list");

    assert!(listed.iter().any(|w| w.path == old.path));
}

#[test]
fn only_worktrees_reported_by_git_can_be_resolved() {
    let fixture = plain_fixture();
    let created = fixture
        .service
        .create_worktree(&fixture.repo.id, "task", None)
        .expect("create");
    let main = fixture.git_repo.path.to_string_lossy().into_owned();

    // Nothing is known until the repo's worktrees have been listed or created.
    fixture
        .service
        .list_worktrees(&fixture.repo.id)
        .expect("list");

    assert_eq!(
        fixture.service.resolve_worktree(&main).expect("main"),
        fixture.git_repo.path
    );
    assert!(fixture.service.resolve_worktree(&created.path).is_ok());
    assert!(matches!(
        fixture.service.resolve_worktree("/etc"),
        Err(AppError::WorktreeNotFound(_))
    ));
    assert!(matches!(
        fixture.service.resolve_worktree(&format!("{main}/src")),
        Err(AppError::WorktreeNotFound(_))
    ));
}

#[test]
fn a_removed_worktree_or_repo_can_no_longer_be_resolved() {
    let fixture = plain_fixture();
    let created = fixture
        .service
        .create_worktree(&fixture.repo.id, "task", None)
        .expect("create");
    assert!(fixture.service.resolve_worktree(&created.path).is_ok());

    fixture
        .service
        .remove_worktree(&fixture.repo.id, &created.path, false)
        .expect("remove");
    assert!(fixture.service.resolve_worktree(&created.path).is_err());

    let main = fixture.git_repo.path.to_string_lossy().into_owned();
    fixture
        .service
        .list_worktrees(&fixture.repo.id)
        .expect("list");
    assert!(fixture.service.resolve_worktree(&main).is_ok());
    fixture
        .service
        .remove_repo(&fixture.repo.id)
        .expect("remove repo");
    assert!(fixture.service.resolve_worktree(&main).is_err());
}

#[test]
fn folders_nest_and_list_in_creation_order() {
    let fixture = plain_fixture();
    let work = fixture.service.create_folder("Work", None).expect("work");
    let client = fixture
        .service
        .create_folder("Client X", Some(&work.id))
        .expect("client");

    let listed = fixture.service.list_folders().expect("list");

    assert_eq!(
        listed.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
        ["Work", "Client X"]
    );
    assert_eq!(
        listed[1].parent_id.as_deref(),
        Some(client.parent_id.as_deref().unwrap())
    );
}

#[test]
fn folder_names_are_trimmed_and_blank_or_long_names_are_refused() {
    let fixture = plain_fixture();
    assert_eq!(
        fixture
            .service
            .create_folder("  Work  ", None)
            .expect("trimmed")
            .name,
        "Work"
    );
    assert!(matches!(
        fixture.service.create_folder("   ", None),
        Err(AppError::InvalidName(_))
    ));
    assert!(matches!(
        fixture.service.create_folder(&"x".repeat(101), None),
        Err(AppError::InvalidName(_))
    ));
    assert!(matches!(
        fixture.service.rename_folder("nope", "Name"),
        Err(AppError::FolderNotFound(_))
    ));
}

#[test]
fn a_folder_cannot_be_moved_into_itself_or_a_folder_inside_it() {
    let fixture = plain_fixture();
    let work = fixture.service.create_folder("Work", None).expect("work");
    let client = fixture
        .service
        .create_folder("Client", Some(&work.id))
        .expect("client");
    let project = fixture
        .service
        .create_folder("Project", Some(&client.id))
        .expect("project");

    assert!(matches!(
        fixture.service.move_folder(&work.id, Some(&work.id)),
        Err(AppError::FolderCycle)
    ));
    assert!(matches!(
        fixture.service.move_folder(&work.id, Some(&project.id)),
        Err(AppError::FolderCycle)
    ));

    // Moving the inner folder out to the top is fine, and then Work can go under it.
    fixture
        .service
        .move_folder(&project.id, None)
        .expect("out to the top");
    fixture
        .service
        .move_folder(&work.id, Some(&project.id))
        .expect("now allowed");
}

#[test]
fn deleting_a_folder_keeps_its_repos_and_worktrees() {
    let fixture = plain_fixture();
    let client = fixture
        .service
        .create_folder("Client", None)
        .expect("client");
    fixture
        .service
        .move_repo(&fixture.repo.id, Some(&client.id))
        .expect("place repo");
    let created = fixture
        .service
        .create_worktree(&fixture.repo.id, "task", None)
        .expect("worktree");

    fixture.service.delete_folder(&client.id).expect("delete");

    assert!(fixture.service.list_folders().expect("list").is_empty());
    assert_eq!(
        fixture.service.list_repos().expect("repos")[0].folder_id,
        None
    );
    assert!(
        Path::new(&created.path).is_dir(),
        "nothing is removed from disk"
    );
    assert!(matches!(
        fixture.service.delete_folder(&client.id),
        Err(AppError::FolderNotFound(_))
    ));
}

#[test]
fn repos_only_go_into_folders_that_exist() {
    let fixture = plain_fixture();
    assert!(matches!(
        fixture.service.move_repo(&fixture.repo.id, Some("ghost")),
        Err(AppError::FolderNotFound(_))
    ));
    assert!(matches!(
        fixture.service.move_repo("ghost", None),
        Err(AppError::RepoNotFound(_))
    ));
}

#[test]
fn folders_and_their_placement_survive_reopening_the_database() {
    let git_repo = TestRepository::new();
    let state_dir = tempfile::tempdir().expect("state dir");
    let service = service_in(&state_dir);
    let repo = service
        .add_repo(&git_repo.path.to_string_lossy())
        .expect("add");
    let work = service.create_folder("Work", None).expect("work");
    service
        .set_folder_collapsed(&work.id, true)
        .expect("collapse");
    service.move_repo(&repo.id, Some(&work.id)).expect("place");

    let reopened = service_in(&state_dir);

    assert!(reopened.list_folders().expect("folders")[0].collapsed);
    assert_eq!(
        reopened.list_repos().expect("repos")[0]
            .folder_id
            .as_deref(),
        Some(work.id.as_str())
    );
}
