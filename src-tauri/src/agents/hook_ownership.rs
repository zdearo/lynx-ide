//! What Lynx wrote into each agent's settings file: its hook commands and the permission rules for its CLI. The book
//! lives in `LYNX_HOME`, not in the agent's file, so another app's entries in that file are never Lynx's to change.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use super::hook_install::{HookInstallError, io_error, read_optional, write_atomic};

/// Serializes the writes: each agent's target updates the same book file.
static WRITES: Mutex<()> = Mutex::new(());

/// What Lynx wrote into one settings file.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Owned {
    #[serde(default)]
    pub commands: Vec<String>,
    #[serde(default)]
    pub rules: Vec<String>,
}

/// `LYNX_HOME/hook-ownership.json`: what Lynx wrote, keyed by the settings file it wrote into.
#[derive(Debug, Clone)]
pub struct OwnershipBook {
    path: PathBuf,
}

impl OwnershipBook {
    pub fn at(path: PathBuf) -> Self {
        Self { path }
    }

    /// What Lynx owns in `file`: what the book names, plus the running app's own command and rule. Those are Lynx's
    /// whatever the book says, so an entry written before the book existed is taken over.
    pub fn owned(&self, file: &Path, command: Option<&str>, rule: Option<&str>) -> Owned {
        let mut owned = self.read().remove(&key(file)).unwrap_or_default();
        owned.commands.extend(command.map(str::to_owned));
        owned.rules.extend(rule.map(str::to_owned));
        owned
    }

    /// Records what Lynx wrote into `file`. Nothing left to record drops the file's entry.
    pub fn record(&self, file: &Path, owned: Owned) -> Result<(), HookInstallError> {
        let _guard = WRITES
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut book = self.read();
        if owned == Owned::default() {
            book.remove(&key(file));
        } else {
            book.insert(key(file), owned);
        }
        let text = serde_json::to_string_pretty(&book)
            .map_err(|error| io_error(&self.path, std::io::Error::other(error)))?;
        write_atomic(&self.path, &format!("{text}\n"))
    }

    /// A missing, unreadable or damaged book names nothing, so Lynx leaves every entry it does not recognize.
    fn read(&self) -> BTreeMap<String, Owned> {
        read_optional(&self.path)
            .ok()
            .flatten()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }
}

fn key(file: &Path) -> String {
    file.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn what_is_recorded_is_owned_again_and_an_empty_record_forgets_the_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let book = OwnershipBook::at(dir.path().join("hook-ownership.json"));
        let file = dir.path().join("settings.json");
        let owned = Owned {
            commands: vec!["'/old/Lynx' --agent-hook claude".to_owned()],
            rules: vec!["Bash(/old/Lynx --cli *)".to_owned()],
        };

        book.record(&file, owned.clone()).expect("record");
        assert_eq!(book.owned(&file, None, None), owned);

        book.record(&file, Owned::default()).expect("forget");
        assert_eq!(book.owned(&file, None, None), Owned::default());
    }

    #[test]
    fn the_running_app_is_owned_even_with_no_book_and_a_damaged_book_names_nothing() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("hook-ownership.json");
        let book = OwnershipBook::at(path.clone());
        let file = dir.path().join("settings.json");
        fs::write(&path, "{ not json").expect("write");

        let owned = book.owned(&file, Some("'/Lynx' --agent-hook claude"), None);

        assert_eq!(owned.commands, vec!["'/Lynx' --agent-hook claude"]);
        assert!(owned.rules.is_empty());
    }
}
