#[cfg(any(unix, test))]
use std::collections::HashSet;
#[cfg(any(unix, test))]
use std::path::{Path, PathBuf};

/// Shells from `/etc/shells` that exist on this machine. Links to the same program
/// (`/bin/bash` and `/usr/bin/bash`) are listed once.
#[cfg(unix)]
pub fn installed_shells() -> Vec<String> {
    let Ok(text) = std::fs::read_to_string("/etc/shells") else {
        return Vec::new();
    };
    parse_shells(&text, |path| std::fs::canonicalize(path).ok())
}

/// Windows has no such list, so the settings offer the login shell and a custom path.
#[cfg(not(unix))]
pub fn installed_shells() -> Vec<String> {
    Vec::new()
}

/// `resolve` returns the real file behind a path, or `None` when nothing is there.
#[cfg(any(unix, test))]
fn parse_shells(text: &str, resolve: impl Fn(&Path) -> Option<PathBuf>) -> Vec<String> {
    let mut seen = HashSet::new();
    text.lines()
        .map(str::trim)
        .filter(|line| line.starts_with('/'))
        .filter(|line| resolve(Path::new(line)).is_some_and(|real| seen.insert(real)))
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn real_files(path: &Path) -> Option<PathBuf> {
        match path.to_str()? {
            "/bin/bash" | "/usr/bin/bash" => Some(PathBuf::from("/usr/bin/bash")),
            "/bin/zsh" => Some(PathBuf::from("/bin/zsh")),
            _ => None,
        }
    }

    #[test]
    fn keeps_absolute_lines_that_exist_and_skips_comments_and_blanks() {
        let text = "# comment\n\n/bin/zsh\n  /bin/bash  \nrelative/shell\n/missing/shell\n";
        assert_eq!(parse_shells(text, real_files), ["/bin/zsh", "/bin/bash"]);
    }

    #[test]
    fn links_to_the_same_program_are_listed_once() {
        let text = "/usr/bin/bash\n/bin/bash\n";
        assert_eq!(parse_shells(text, real_files), ["/usr/bin/bash"]);
    }
}
