use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use super::linked_folder_scan::ScanTree;
use super::workspace_models::{Folder, Repo};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewFolder {
    pub id: String,
    pub name: String,
    pub parent_id: String,
    pub source_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewRepo {
    pub id: String,
    pub name: String,
    pub path: String,
    pub folder_id: String,
}

/// What one scan changes: what to create, and which known items to flag or unflag as missing.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct LinkedPlan {
    pub root_id: String,
    /// A parent comes before its children.
    pub new_folders: Vec<NewFolder>,
    pub new_repos: Vec<NewRepo>,
    pub folder_missing: Vec<(String, bool)>,
    pub repo_missing: Vec<(String, bool)>,
}

impl LinkedPlan {
    pub fn is_empty(&self) -> bool {
        self.new_folders.is_empty()
            && self.new_repos.is_empty()
            && self.folder_missing.is_empty()
            && self.repo_missing.is_empty()
    }
}

pub struct PlanInput<'a> {
    pub root: &'a Folder,
    /// `None` when the linked directory itself is gone.
    pub scan: Option<&'a ScanTree>,
    pub folders: &'a [Folder],
    pub repos: &'a [Repo],
    /// `canonical_key` of every registered repo, so a repo added by hand is not added twice.
    pub taken: &'a HashSet<String>,
    /// Repos of this linked folder that the user removed.
    pub excluded: &'a HashSet<String>,
}

/// Compares what a scan found with what is stored. It only adds and flags: it never moves a
/// folder or repo that exists, so where the user put things stays put, and it never deletes.
pub fn plan(input: &PlanInput<'_>) -> LinkedPlan {
    let root = input.root;
    let mut plan = LinkedPlan {
        root_id: root.id.clone(),
        ..LinkedPlan::default()
    };

    // Items this linked folder already owns, by the directory they mirror.
    let known_folders: HashMap<&str, &Folder> = input
        .folders
        .iter()
        .filter(|folder| folder.linked_root_id.as_deref() == Some(root.id.as_str()))
        .filter_map(|folder| Some((folder.source_path.as_deref()?, folder)))
        .collect();
    let known_repos: HashMap<&str, &Repo> = input
        .repos
        .iter()
        .filter(|repo| repo.linked_root_id.as_deref() == Some(root.id.as_str()))
        .map(|repo| (repo.path.as_str(), repo))
        .collect();

    let Some(scan) = input.scan else {
        // The disk may just be unmounted: flag everything and keep it.
        for folder in known_folders.values().filter(|folder| !folder.missing) {
            plan.folder_missing.push((folder.id.clone(), true));
        }
        for repo in known_repos.values().filter(|repo| !repo.missing) {
            plan.repo_missing.push((repo.id.clone(), true));
        }
        return plan;
    };

    if root.missing {
        plan.folder_missing.push((root.id.clone(), false));
    }

    let mut folder_ids: HashMap<String, String> = known_folders
        .iter()
        .map(|(path, folder)| ((*path).to_owned(), folder.id.clone()))
        .collect();
    for directory in &scan.directories {
        let path = path_string(&directory.path);
        if folder_ids.contains_key(&path) {
            continue;
        }
        let Some(parent_id) = folder_ids.get(&path_string(&directory.parent)).cloned() else {
            continue;
        };
        let id = uuid::Uuid::new_v4().to_string();
        folder_ids.insert(path.clone(), id.clone());
        plan.new_folders.push(NewFolder {
            id,
            name: directory_name(&directory.path),
            parent_id,
            source_path: path,
        });
    }

    for repo in &scan.repos {
        let path = path_string(&repo.path);
        if known_repos.contains_key(path.as_str())
            || input.excluded.contains(&path)
            || input.taken.contains(&canonical_key(&repo.path))
        {
            continue;
        }
        let Some(folder_id) = folder_ids.get(&path_string(&repo.parent)).cloned() else {
            continue;
        };
        plan.new_repos.push(NewRepo {
            id: uuid::Uuid::new_v4().to_string(),
            name: directory_name(&repo.path),
            path,
            folder_id,
        });
    }

    let found_directories: HashSet<String> = scan
        .directories
        .iter()
        .map(|directory| path_string(&directory.path))
        .collect();
    let found_repos: HashSet<String> = scan
        .repos
        .iter()
        .map(|repo| path_string(&repo.path))
        .collect();
    // A cut-short scan cannot prove something is gone, only that it is back.
    for (path, folder) in &known_folders {
        if folder.id == root.id {
            continue;
        }
        let found = found_directories.contains(*path);
        if found == folder.missing && (found || !scan.truncated) {
            plan.folder_missing.push((folder.id.clone(), !found));
        }
    }
    for (path, repo) in &known_repos {
        let found = found_repos.contains(*path);
        if found == repo.missing && (found || !scan.truncated) {
            plan.repo_missing.push((repo.id.clone(), !found));
        }
    }
    plan
}

/// A path as one comparable string: symlinks resolved and, on Windows, no `\\?\` prefix. Git
/// reports a repo's path in its own spelling, so two spellings of one repo must compare equal.
pub fn canonical_key(path: &Path) -> String {
    let resolved = fs::canonicalize(path).unwrap_or_else(|_| path.to_owned());
    path_string(&without_verbatim_prefix(resolved))
}

/// `fs::canonicalize` on Windows returns `\\?\C:\...`, which git and the dialogs never print.
pub fn without_verbatim_prefix(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC\\") => PathBuf::from(rest),
        _ => path,
    }
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn directory_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path_string(path),
        |name| name.to_string_lossy().into_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::super::linked_folder_scan::{ScannedDirectory, ScannedRepo};
    use super::*;

    const ROOT: &str = "/code";

    fn root() -> Folder {
        Folder {
            id: "root".to_owned(),
            name: "Code".to_owned(),
            parent_id: None,
            collapsed: false,
            source_path: Some(ROOT.to_owned()),
            linked_root_id: Some("root".to_owned()),
            missing: false,
        }
    }

    fn mirrored(id: &str, path: &str, parent: &str, missing: bool) -> Folder {
        Folder {
            id: id.to_owned(),
            name: id.to_owned(),
            parent_id: Some(parent.to_owned()),
            collapsed: false,
            source_path: Some(path.to_owned()),
            linked_root_id: Some("root".to_owned()),
            missing,
        }
    }

    fn linked_repo(id: &str, path: &str, folder: &str, missing: bool) -> Repo {
        Repo {
            id: id.to_owned(),
            name: id.to_owned(),
            path: path.to_owned(),
            folder_id: Some(folder.to_owned()),
            linked_root_id: Some("root".to_owned()),
            missing,
        }
    }

    fn scan_of(directories: &[(&str, &str)], repos: &[(&str, &str)]) -> ScanTree {
        ScanTree {
            directories: directories
                .iter()
                .map(|(path, parent)| ScannedDirectory {
                    path: PathBuf::from(path),
                    parent: PathBuf::from(parent),
                })
                .collect(),
            repos: repos
                .iter()
                .map(|(path, parent)| ScannedRepo {
                    path: PathBuf::from(path),
                    parent: PathBuf::from(parent),
                })
                .collect(),
            truncated: false,
        }
    }

    fn run(
        scan: Option<&ScanTree>,
        folders: &[Folder],
        repos: &[Repo],
        taken: &[&str],
        excluded: &[&str],
    ) -> LinkedPlan {
        let root = root();
        let mut all_folders = vec![root.clone()];
        all_folders.extend_from_slice(folders);
        plan(&PlanInput {
            root: &root,
            scan,
            folders: &all_folders,
            repos,
            taken: &taken.iter().map(|path| (*path).to_owned()).collect(),
            excluded: &excluded.iter().map(|path| (*path).to_owned()).collect(),
        })
    }

    #[test]
    fn creates_folders_and_repos_under_the_right_parents() {
        let scan = scan_of(
            &[
                ("/code/clients", "/code"),
                ("/code/clients/beta", "/code/clients"),
            ],
            &[
                ("/code/lynx", "/code"),
                ("/code/clients/acme", "/code/clients"),
                ("/code/clients/beta/api", "/code/clients/beta"),
            ],
        );

        let plan = run(Some(&scan), &[], &[], &[], &[]);

        let clients = &plan.new_folders[0];
        let beta = &plan.new_folders[1];
        assert_eq!(clients.parent_id, "root");
        assert_eq!(beta.parent_id, clients.id);
        assert_eq!(beta.name, "beta");
        let parent_of = |name: &str| {
            plan.new_repos
                .iter()
                .find(|repo| repo.name == name)
                .map(|repo| repo.folder_id.clone())
        };
        assert_eq!(parent_of("lynx").as_deref(), Some("root"));
        assert_eq!(parent_of("acme").as_deref(), Some(clients.id.as_str()));
        assert_eq!(parent_of("api").as_deref(), Some(beta.id.as_str()));
    }

    #[test]
    fn nothing_new_means_an_empty_plan() {
        let scan = scan_of(
            &[("/code/clients", "/code")],
            &[("/code/clients/acme", "/code/clients")],
        );
        let folders = [mirrored("clients", "/code/clients", "root", false)];
        let repos = [linked_repo("acme", "/code/clients/acme", "clients", false)];

        assert!(run(Some(&scan), &folders, &repos, &[], &[]).is_empty());
    }

    #[test]
    fn leaves_alone_where_the_user_moved_things() {
        // The repo was moved out of `clients` into another folder; the scan still sees it there.
        let scan = scan_of(
            &[("/code/clients", "/code")],
            &[("/code/clients/acme", "/code/clients")],
        );
        let folders = [mirrored("clients", "/code/clients", "root", false)];
        let mut moved = linked_repo("acme", "/code/clients/acme", "clients", false);
        moved.folder_id = Some("somewhere-else".to_owned());

        assert!(run(Some(&scan), &folders, &[moved], &[], &[]).is_empty());
    }

    #[test]
    fn skips_repos_that_were_added_by_hand_or_removed_by_the_user() {
        let scan = scan_of(
            &[],
            &[
                ("/code/mine", "/code"),
                ("/code/gone", "/code"),
                ("/code/new", "/code"),
            ],
        );

        let plan = run(Some(&scan), &[], &[], &["/code/mine"], &["/code/gone"]);

        assert_eq!(plan.new_repos.len(), 1);
        assert_eq!(plan.new_repos[0].name, "new");
    }

    #[test]
    fn flags_what_is_no_longer_found_and_unflags_what_came_back() {
        let scan = scan_of(
            &[("/code/back", "/code")],
            &[("/code/back/repo", "/code/back")],
        );
        let folders = [
            mirrored("back", "/code/back", "root", true),
            mirrored("old", "/code/old", "root", false),
        ];
        let repos = [
            linked_repo("repo", "/code/back/repo", "back", true),
            linked_repo("lost", "/code/old/lost", "old", false),
        ];

        let plan = run(Some(&scan), &folders, &repos, &[], &[]);

        let folder_flags: HashMap<_, _> = plan.folder_missing.iter().cloned().collect();
        let repo_flags: HashMap<_, _> = plan.repo_missing.iter().cloned().collect();
        assert_eq!(folder_flags.get("back"), Some(&false));
        assert_eq!(folder_flags.get("old"), Some(&true));
        assert_eq!(repo_flags.get("repo"), Some(&false));
        assert_eq!(repo_flags.get("lost"), Some(&true));
    }

    #[test]
    fn a_truncated_scan_never_flags_anything_as_missing() {
        let mut scan = scan_of(&[], &[]);
        scan.truncated = true;
        let folders = [mirrored("old", "/code/old", "root", false)];
        let repos = [linked_repo("lost", "/code/old/lost", "old", false)];

        assert!(run(Some(&scan), &folders, &repos, &[], &[]).is_empty());
    }

    #[test]
    fn a_missing_directory_flags_everything_and_deletes_nothing() {
        let folders = [mirrored("clients", "/code/clients", "root", false)];
        let repos = [linked_repo("acme", "/code/clients/acme", "clients", false)];

        let plan = run(None, &folders, &repos, &[], &[]);

        assert!(plan.new_folders.is_empty() && plan.new_repos.is_empty());
        let flagged: HashSet<_> = plan
            .folder_missing
            .iter()
            .map(|(id, _)| id.as_str())
            .collect();
        assert_eq!(flagged, HashSet::from(["root", "clients"]));
        assert_eq!(plan.repo_missing, [("acme".to_owned(), true)]);
    }

    #[test]
    fn the_linked_folder_comes_back_when_its_directory_does() {
        let mut root = root();
        root.missing = true;
        let scan = scan_of(&[], &[]);

        let plan = super::plan(&PlanInput {
            root: &root,
            scan: Some(&scan),
            folders: std::slice::from_ref(&root),
            repos: &[],
            taken: &HashSet::new(),
            excluded: &HashSet::new(),
        });

        assert_eq!(plan.folder_missing, [("root".to_owned(), false)]);
    }

    #[test]
    fn canonical_key_treats_spellings_of_one_directory_alike() {
        let dir = tempfile::tempdir().expect("dir");
        let direct = dir.path().to_owned();
        let roundabout = dir.path().join("sub").join("..");
        fs::create_dir(dir.path().join("sub")).expect("sub");

        assert_eq!(canonical_key(&direct), canonical_key(&roundabout));
    }

    #[test]
    fn strips_the_verbatim_prefix_but_not_unc_paths() {
        assert_eq!(
            without_verbatim_prefix(PathBuf::from(r"\\?\C:\work\app")),
            PathBuf::from(r"C:\work\app")
        );
        assert_eq!(
            without_verbatim_prefix(PathBuf::from(r"\\?\UNC\server\share")),
            PathBuf::from(r"\\?\UNC\server\share")
        );
        assert_eq!(
            without_verbatim_prefix(PathBuf::from("/home/user")),
            PathBuf::from("/home/user")
        );
    }
}
