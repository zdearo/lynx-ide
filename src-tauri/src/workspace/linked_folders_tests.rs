use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tempfile::TempDir;

use crate::error::AppError;
use crate::git::git_command::git;
use crate::persistence::AppDatabase;

use super::linked_folder_sync::without_verbatim_prefix;
use super::linked_folders::LinkedFolderError;
use super::workspace_models::{Folder, Repo};
use super::workspace_service::WorkspaceService;

struct Setup {
    service: WorkspaceService,
    /// The directory that gets linked, already canonical so paths compare on every platform.
    code: PathBuf,
    _code_dir: TempDir,
    _state_dir: TempDir,
}

fn init_repo(path: &Path) {
    fs::create_dir_all(path).expect("make repo dir");
    git(path).args(["init", "--quiet"]).run().expect("git init");
}

/// ~/code with `lynx`, `clients/acme` and `clients/beta/api` as repos, and a `notes` folder.
fn setup() -> Setup {
    let code_dir = tempfile::tempdir().expect("code dir");
    let code = without_verbatim_prefix(fs::canonicalize(code_dir.path()).expect("canonical"));
    init_repo(&code.join("lynx"));
    init_repo(&code.join("clients/acme"));
    init_repo(&code.join("clients/beta/api"));
    fs::create_dir_all(code.join("notes/ideas")).expect("notes");

    let state_dir = tempfile::tempdir().expect("state dir");
    let database = AppDatabase::open(&state_dir.path().join("app.db")).expect("open database");
    Setup {
        service: WorkspaceService::new(database, Arc::new(|| None)),
        code,
        _code_dir: code_dir,
        _state_dir: state_dir,
    }
}

fn link(setup: &Setup) -> Folder {
    setup
        .service
        .link_folder("Code", None, &setup.code.to_string_lossy())
        .expect("link")
}

fn folder_named(setup: &Setup, name: &str) -> Folder {
    setup
        .service
        .list_folders()
        .expect("folders")
        .into_iter()
        .find(|folder| folder.name == name)
        .unwrap_or_else(|| panic!("no folder named {name}"))
}

fn repo_named(setup: &Setup, name: &str) -> Repo {
    setup
        .service
        .list_repos()
        .expect("repos")
        .into_iter()
        .find(|repo| repo.name == name)
        .unwrap_or_else(|| panic!("no repo named {name}"))
}

fn has_repo(setup: &Setup, name: &str) -> bool {
    setup
        .service
        .list_repos()
        .expect("repos")
        .iter()
        .any(|repo| repo.name == name)
}

#[test]
fn linking_mirrors_directories_as_folders_and_the_first_git_repo_as_a_project() {
    let setup = setup();

    let root = link(&setup);

    let clients = folder_named(&setup, "clients");
    let beta = folder_named(&setup, "beta");
    assert_eq!(clients.parent_id.as_deref(), Some(root.id.as_str()));
    assert_eq!(beta.parent_id.as_deref(), Some(clients.id.as_str()));
    assert_eq!(
        repo_named(&setup, "lynx").folder_id.as_deref(),
        Some(root.id.as_str())
    );
    assert_eq!(
        repo_named(&setup, "acme").folder_id.as_deref(),
        Some(clients.id.as_str())
    );
    assert_eq!(
        repo_named(&setup, "api").folder_id.as_deref(),
        Some(beta.id.as_str())
    );
    // A folder with no repository below it is not mirrored.
    assert!(
        setup
            .service
            .list_folders()
            .expect("folders")
            .iter()
            .all(|f| f.name != "notes")
    );
    assert!(root.is_linked_root());
    assert!(clients.is_mirrored());
    assert_eq!(
        repo_named(&setup, "lynx").linked_root_id.as_deref(),
        Some(root.id.as_str())
    );
}

#[test]
fn syncing_again_changes_nothing() {
    let setup = setup();
    link(&setup);

    let changed = setup.service.sync_linked_folders(None).expect("sync");

    assert!(!changed);
    assert_eq!(setup.service.list_repos().expect("repos").len(), 3);
    assert_eq!(setup.service.list_folders().expect("folders").len(), 3);
}

#[test]
fn a_repo_cloned_later_shows_up_in_the_right_folder() {
    let setup = setup();
    link(&setup);
    init_repo(&setup.code.join("clients/gamma"));
    init_repo(&setup.code.join("notes/ideas/spike"));

    let changed = setup.service.sync_linked_folders(None).expect("sync");

    assert!(changed);
    let clients = folder_named(&setup, "clients");
    assert_eq!(
        repo_named(&setup, "gamma").folder_id.as_deref(),
        Some(clients.id.as_str())
    );
    let ideas = folder_named(&setup, "ideas");
    assert_eq!(
        folder_named(&setup, "notes").id,
        ideas.parent_id.expect("parent")
    );
    assert_eq!(
        repo_named(&setup, "spike").folder_id.as_deref(),
        Some(ideas.id.as_str())
    );
}

#[test]
fn where_the_user_moved_a_repo_is_kept() {
    let setup = setup();
    link(&setup);
    let personal = setup
        .service
        .create_folder("Personal", None)
        .expect("folder");
    let acme = repo_named(&setup, "acme");
    setup
        .service
        .move_repo(&acme.id, Some(&personal.id))
        .expect("move");

    setup.service.sync_linked_folders(None).expect("sync");

    assert_eq!(
        repo_named(&setup, "acme").folder_id.as_deref(),
        Some(personal.id.as_str())
    );
    assert_eq!(setup.service.list_repos().expect("repos").len(), 3);
}

#[test]
fn a_repo_the_user_removed_stays_out_of_later_scans() {
    let setup = setup();
    link(&setup);
    let lynx = repo_named(&setup, "lynx");

    setup.service.remove_repo(&lynx.id).expect("remove");
    setup.service.sync_linked_folders(None).expect("sync");

    assert!(!has_repo(&setup, "lynx"));
    assert!(
        fs::metadata(setup.code.join("lynx/.git")).is_ok(),
        "disk untouched"
    );
}

#[test]
fn what_disappears_from_disk_is_flagged_not_deleted_and_comes_back_unflagged() {
    let setup = setup();
    link(&setup);
    let parked = setup.code.parent().expect("parent").join("parked-for-test");
    fs::rename(setup.code.join("clients"), &parked).expect("move away");

    let changed = setup.service.sync_linked_folders(None).expect("sync");

    assert!(changed);
    assert!(repo_named(&setup, "acme").missing);
    assert!(repo_named(&setup, "api").missing);
    assert!(folder_named(&setup, "clients").missing);
    assert!(!repo_named(&setup, "lynx").missing);

    fs::rename(&parked, setup.code.join("clients")).expect("move back");
    setup.service.sync_linked_folders(None).expect("sync");

    assert!(!repo_named(&setup, "acme").missing);
    assert!(!folder_named(&setup, "clients").missing);
}

#[test]
fn an_unmounted_directory_flags_everything_and_removes_nothing() {
    let setup = setup();
    let root = link(&setup);
    let parked = setup
        .code
        .parent()
        .expect("parent")
        .join("unmounted-for-test");
    fs::rename(&setup.code, &parked).expect("unmount");

    setup.service.sync_linked_folders(None).expect("sync");

    assert_eq!(setup.service.list_repos().expect("repos").len(), 3);
    assert!(
        setup
            .service
            .list_repos()
            .expect("repos")
            .iter()
            .all(|repo| repo.missing)
    );
    assert!(folder_named(&setup, "Code").missing);

    fs::rename(&parked, &setup.code).expect("remount");
    setup
        .service
        .sync_linked_folders(Some(&root.id))
        .expect("sync");

    assert!(
        setup
            .service
            .list_repos()
            .expect("repos")
            .iter()
            .all(|repo| !repo.missing)
    );
    assert!(!folder_named(&setup, "Code").missing);
}

#[test]
fn a_repo_added_by_hand_is_not_claimed_or_duplicated() {
    let setup = setup();
    let by_hand = setup
        .service
        .add_repo(&setup.code.join("lynx").to_string_lossy())
        .expect("add by hand");

    link(&setup);

    let repos = setup.service.list_repos().expect("repos");
    assert_eq!(repos.iter().filter(|repo| repo.name == "lynx").count(), 1);
    let lynx = repo_named(&setup, "lynx");
    assert_eq!(lynx.id, by_hand.id);
    assert_eq!(lynx.folder_id, None);
    assert_eq!(lynx.linked_root_id, None);
}

#[test]
fn adding_a_linked_repo_by_hand_returns_the_one_that_exists() {
    let setup = setup();
    link(&setup);
    let found = repo_named(&setup, "acme");

    let added = setup
        .service
        .add_repo(&setup.code.join("clients/acme").to_string_lossy())
        .expect("add");

    assert_eq!(added.id, found.id);
    assert_eq!(setup.service.list_repos().expect("repos").len(), 3);
}

#[test]
fn a_linked_worktree_beside_its_repo_is_not_a_second_project() {
    let setup = setup();
    let lynx = setup.code.join("lynx");
    git(&lynx)
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "--allow-empty",
            "-m",
            "init",
            "--quiet",
        ])
        .run()
        .expect("commit");
    git(&lynx)
        .args(["worktree", "add", "--quiet", "-b", "feature"])
        .arg(setup.code.join("lynx-worktrees/feature"))
        .run()
        .expect("worktree");

    link(&setup);

    assert_eq!(setup.service.list_repos().expect("repos").len(), 3);
    assert!(
        setup
            .service
            .list_folders()
            .expect("folders")
            .iter()
            .all(|f| f.name != "lynx-worktrees")
    );
}

#[test]
fn unlinking_keeps_everything_as_ordinary_folders_and_stops_syncing() {
    let setup = setup();
    let root = link(&setup);

    setup.service.unlink_folder(&root.id).expect("unlink");
    init_repo(&setup.code.join("clients/gamma"));
    let changed = setup.service.sync_linked_folders(None).expect("sync");

    assert!(!changed);
    assert!(!has_repo(&setup, "gamma"));
    assert_eq!(setup.service.list_repos().expect("repos").len(), 3);
    let folders = setup.service.list_folders().expect("folders");
    assert!(
        folders
            .iter()
            .all(|folder| folder.linked_root_id.is_none() && folder.source_path.is_none())
    );
    assert!(
        setup
            .service
            .list_repos()
            .expect("repos")
            .iter()
            .all(|repo| repo.linked_root_id.is_none())
    );
    // Ordinary folders can be deleted again.
    setup
        .service
        .delete_folder(&folder_named(&setup, "clients").id)
        .expect("delete");
}

#[test]
fn deleting_the_linked_folder_unlinks_and_keeps_its_projects() {
    let setup = setup();
    let root = link(&setup);

    setup.service.delete_folder(&root.id).expect("delete");

    assert_eq!(setup.service.list_repos().expect("repos").len(), 3);
    assert!(!setup.service.sync_linked_folders(None).expect("sync"));
    assert_eq!(repo_named(&setup, "lynx").folder_id, None);
    assert_eq!(repo_named(&setup, "lynx").linked_root_id, None);
}

#[test]
fn a_mirrored_folder_cannot_be_deleted_or_unlinked_on_its_own() {
    let setup = setup();
    link(&setup);
    let clients = folder_named(&setup, "clients");

    assert!(matches!(
        setup.service.delete_folder(&clients.id),
        Err(AppError::LinkedFolder(LinkedFolderError::Managed))
    ));
    assert!(matches!(
        setup.service.unlink_folder(&clients.id),
        Err(AppError::LinkedFolder(LinkedFolderError::NotLinked))
    ));
}

#[test]
fn a_mirrored_folder_can_still_be_renamed_and_moved() {
    let setup = setup();
    link(&setup);
    let personal = setup
        .service
        .create_folder("Personal", None)
        .expect("folder");
    let clients = folder_named(&setup, "clients");

    setup
        .service
        .rename_folder(&clients.id, "Customers")
        .expect("rename");
    setup
        .service
        .move_folder(&clients.id, Some(&personal.id))
        .expect("move");
    setup.service.sync_linked_folders(None).expect("sync");

    let moved = folder_named(&setup, "Customers");
    assert_eq!(moved.parent_id.as_deref(), Some(personal.id.as_str()));
    assert_eq!(setup.service.list_folders().expect("folders").len(), 4);
}

#[test]
fn refuses_a_directory_that_cannot_be_linked() {
    let setup = setup();
    let root = link(&setup);

    let repo_itself =
        setup
            .service
            .link_folder("x", None, &setup.code.join("lynx").to_string_lossy());
    let same = setup
        .service
        .link_folder("x", None, &setup.code.to_string_lossy());
    let inside = setup
        .service
        .link_folder("x", None, &setup.code.join("notes").to_string_lossy());
    let above = setup.service.link_folder(
        "x",
        None,
        &setup.code.parent().expect("parent").to_string_lossy(),
    );
    let missing = setup
        .service
        .link_folder("x", None, &setup.code.join("nope").to_string_lossy());
    let file = setup.code.join("file.txt");
    fs::write(&file, "x").expect("file");
    let not_a_directory = setup
        .service
        .link_folder("x", None, &file.to_string_lossy());
    let blank = setup
        .service
        .link_folder("  ", None, &setup.code.join("notes").to_string_lossy());

    assert!(matches!(
        repo_itself,
        Err(AppError::LinkedFolder(LinkedFolderError::IsRepository(_)))
    ));
    assert!(matches!(
        same,
        Err(AppError::LinkedFolder(LinkedFolderError::Overlaps(_)))
    ));
    assert!(matches!(
        inside,
        Err(AppError::LinkedFolder(LinkedFolderError::Overlaps(_)))
    ));
    assert!(matches!(
        above,
        Err(AppError::LinkedFolder(LinkedFolderError::Overlaps(_)))
    ));
    assert!(matches!(missing, Err(AppError::LocationNotFound(_))));
    assert!(matches!(
        not_a_directory,
        Err(AppError::LocationNotADirectory(_))
    ));
    assert!(matches!(blank, Err(AppError::InvalidName(_))));
    // Nothing was left behind by the refusals.
    assert_eq!(setup.service.list_folders().expect("folders").len(), 3);
    assert!(root.is_linked_root());
}

#[test]
fn previewing_counts_without_creating_anything() {
    let setup = setup();

    let preview = setup
        .service
        .preview_linked_folder(&setup.code.to_string_lossy())
        .expect("preview");

    assert_eq!(preview.projects, 3);
    assert_eq!(preview.folders, 2);
    assert!(!preview.truncated);
    assert!(setup.service.list_folders().expect("folders").is_empty());
    assert!(setup.service.list_repos().expect("repos").is_empty());
}

#[test]
fn syncing_an_unknown_or_unlinked_folder_is_an_error() {
    let setup = setup();
    let ordinary = setup.service.create_folder("Plain", None).expect("folder");

    assert!(matches!(
        setup.service.sync_linked_folders(Some("missing")),
        Err(AppError::FolderNotFound(_))
    ));
    assert!(matches!(
        setup.service.sync_linked_folders(Some(&ordinary.id)),
        Err(AppError::FolderNotFound(_))
    ));
    assert!(
        !setup
            .service
            .sync_linked_folders(None)
            .expect("nothing linked")
    );
}

#[test]
fn links_and_their_flags_survive_reopening_the_database() {
    let code_dir = tempfile::tempdir().expect("code dir");
    let code = without_verbatim_prefix(fs::canonicalize(code_dir.path()).expect("canonical"));
    init_repo(&code.join("app"));
    let state_dir = tempfile::tempdir().expect("state dir");
    let open = || {
        let database = AppDatabase::open(&state_dir.path().join("app.db")).expect("open");
        WorkspaceService::new(database, Arc::new(|| None))
    };
    let first = open();
    let root = first
        .link_folder("Code", None, &code.to_string_lossy())
        .expect("link");
    drop(first);

    let second = open();

    let folders = second.list_folders().expect("folders");
    assert_eq!(folders.len(), 1);
    assert!(folders[0].is_linked_root());
    assert_eq!(
        folders[0].source_path.as_deref(),
        Some(code.to_string_lossy().as_ref())
    );
    assert_eq!(
        second.list_repos().expect("repos")[0]
            .linked_root_id
            .as_deref(),
        Some(root.id.as_str())
    );
    assert!(!second.sync_linked_folders(None).expect("sync"));
}
