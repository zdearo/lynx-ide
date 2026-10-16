use std::collections::HashMap;

use super::workspace_models::Folder;

/// True when moving `moving` under `new_parent` would make a folder its own
/// ancestor. A chain longer than the folder list means the data already loops;
/// refusing is safer than walking it forever.
pub fn would_create_cycle(folders: &[Folder], moving: &str, new_parent: Option<&str>) -> bool {
    let parents: HashMap<&str, Option<&str>> = folders
        .iter()
        .map(|folder| (folder.id.as_str(), folder.parent_id.as_deref()))
        .collect();

    let mut current = new_parent;
    let mut steps = 0;
    while let Some(id) = current {
        if id == moving {
            return true;
        }
        steps += 1;
        if steps > folders.len() {
            return true;
        }
        current = parents.get(id).copied().flatten();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(id: &str, parent: Option<&str>) -> Folder {
        Folder {
            id: id.to_owned(),
            name: id.to_owned(),
            parent_id: parent.map(str::to_owned),
            collapsed: false,
        }
    }

    // Work > Client > Project, and a sibling Personal at the top.
    fn tree() -> Vec<Folder> {
        vec![
            folder("work", None),
            folder("client", Some("work")),
            folder("project", Some("client")),
            folder("personal", None),
        ]
    }

    #[test]
    fn moving_to_the_top_level_never_cycles() {
        assert!(!would_create_cycle(&tree(), "project", None));
    }

    #[test]
    fn moving_under_an_unrelated_folder_is_fine() {
        assert!(!would_create_cycle(&tree(), "client", Some("personal")));
        assert!(!would_create_cycle(&tree(), "personal", Some("work")));
    }

    #[test]
    fn a_folder_cannot_go_under_itself() {
        assert!(would_create_cycle(&tree(), "work", Some("work")));
    }

    #[test]
    fn a_folder_cannot_go_under_any_of_its_descendants() {
        assert!(would_create_cycle(&tree(), "work", Some("client")));
        assert!(would_create_cycle(&tree(), "work", Some("project")));
        assert!(would_create_cycle(&tree(), "client", Some("project")));
    }

    #[test]
    fn a_corrupt_loop_is_refused_instead_of_walked_forever() {
        let looped = vec![
            folder("a", Some("b")),
            folder("b", Some("a")),
            folder("c", None),
        ];
        assert!(would_create_cycle(&looped, "c", Some("a")));
    }
}
