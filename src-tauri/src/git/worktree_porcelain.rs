use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PorcelainWorktree {
    pub path: PathBuf,
    pub head: Option<String>,
    /// Short branch name; `None` when HEAD is detached or the repo is bare.
    pub branch: Option<String>,
    pub bare: bool,
    pub locked: bool,
    pub prunable: bool,
}

/// Parses `git worktree list --porcelain` (stable since Git 2.7). The first
/// entry is always the main worktree.
pub fn parse_worktree_list(output: &str) -> Vec<PorcelainWorktree> {
    output
        .split("\n\n")
        .filter_map(|block| parse_block(block.trim_matches(['\n', '\r'])))
        .collect()
}

fn parse_block(block: &str) -> Option<PorcelainWorktree> {
    let mut worktree = PorcelainWorktree {
        path: PathBuf::new(),
        head: None,
        branch: None,
        bare: false,
        locked: false,
        prunable: false,
    };
    let mut has_path = false;

    for line in block.lines() {
        let (key, value) = line.split_once(' ').unwrap_or((line, ""));
        match key {
            "worktree" => {
                worktree.path = PathBuf::from(value);
                has_path = true;
            }
            "HEAD" => worktree.head = Some(value.to_owned()),
            "branch" => {
                worktree.branch = Some(
                    value
                        .strip_prefix("refs/heads/")
                        .unwrap_or(value)
                        .to_owned(),
                )
            }
            "bare" => worktree.bare = true,
            "locked" => worktree.locked = true,
            "prunable" => worktree.prunable = true,
            _ => {}
        }
    }

    has_path.then_some(worktree)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_main_branch_detached_locked_and_prunable_entries() {
        let output = "\
worktree /work/repo
HEAD 1111111111111111111111111111111111111111
branch refs/heads/main

worktree /work/repo-worktrees/feature-x
HEAD 2222222222222222222222222222222222222222
branch refs/heads/feature/x
locked reason text

worktree /work/repo-worktrees/detached
HEAD 3333333333333333333333333333333333333333
detached

worktree /work/gone
HEAD 4444444444444444444444444444444444444444
branch refs/heads/gone
prunable gitdir file points to non-existent location
";
        let parsed = parse_worktree_list(output);

        assert_eq!(parsed.len(), 4);
        assert_eq!(parsed[0].path, PathBuf::from("/work/repo"));
        assert_eq!(parsed[0].branch.as_deref(), Some("main"));
        assert_eq!(parsed[1].branch.as_deref(), Some("feature/x"));
        assert!(parsed[1].locked);
        assert_eq!(parsed[2].branch, None);
        assert!(parsed[3].prunable);
    }

    #[test]
    fn parses_bare_repository() {
        let parsed = parse_worktree_list("worktree /srv/repo.git\nbare\n");
        assert_eq!(parsed.len(), 1);
        assert!(parsed[0].bare);
        assert_eq!(parsed[0].head, None);
    }

    #[test]
    fn ignores_empty_output() {
        assert!(parse_worktree_list("").is_empty());
    }
}
