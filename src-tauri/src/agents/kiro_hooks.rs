//! Kiro status: Kiro keeps hooks in each agent's own file, not in one global file. So Lynx adds the
//! same entries to every agent file in Kiro's agents folder, and says so before it writes.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::hook_install::{HookInstallError, HookInstallState, HookInstallStatus, io_error};
use super::json_hook_file::{self, EventSpec, JsonHookFormat, JsonHookTarget};

const FORMAT: JsonHookFormat = JsonHookFormat {
    root_key: "hooks",
    version: None,
    event_argument: true,
    handler: kiro_handler,
};

const EVENTS: &[EventSpec] = &[
    EventSpec::flat("agentSpawn", None),
    EventSpec::flat("userPromptSubmit", None),
    EventSpec::flat("preToolUse", Some("*")),
    EventSpec::flat("postToolUse", Some("*")),
    EventSpec::flat("stop", None),
];

fn kiro_handler(command: &str) -> Value {
    json!({ "command": command, "timeout_ms": 5000 })
}

#[derive(Clone)]
pub struct KiroHookTarget {
    /// Kiro's agents folder: `KIRO_HOME/agents`, or `~/.kiro/agents`.
    folder: PathBuf,
    executable: Option<PathBuf>,
}

impl KiroHookTarget {
    pub fn new(folder: PathBuf, executable: Option<&Path>) -> Self {
        Self {
            folder,
            executable: executable.map(Path::to_path_buf),
        }
    }

    /// The agent files in the folder, in name order so the status reads the same each time.
    fn files(&self) -> Result<Vec<JsonHookTarget>, HookInstallError> {
        let entries = match fs::read_dir(&self.folder) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => return Err(io_error(&self.folder, source)),
        };
        let mut paths: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "json")
            })
            .filter(|path| path.is_file())
            .collect();
        paths.sort();
        Ok(paths
            .into_iter()
            .map(|path| {
                JsonHookTarget::new("kiro", path, FORMAT, EVENTS, self.executable.as_deref())
            })
            .collect())
    }

    pub fn status(&self) -> HookInstallStatus {
        match self.files() {
            Ok(files) => self.describe(&files),
            Err(error) => self.status_with(HookInstallState::Error, Some(error.to_string())),
        }
    }

    /// Adds the entries to every agent file. Each file is read first, so one that Lynx cannot read
    /// stops the change before any file is written.
    pub fn install(&self) -> Result<HookInstallStatus, HookInstallError> {
        let files = self.files()?;
        if files.is_empty() {
            return Err(HookInstallError::NoAgentFiles {
                path: self.folder.to_string_lossy().into_owned(),
            });
        }
        for file in &files {
            json_hook_file::read_settings(&file.path, &FORMAT)?;
        }
        for file in &files {
            file.install()?;
        }
        Ok(self.status())
    }

    /// Removes the entries from every agent file, leaving the rest of each file as it was.
    pub fn remove(&self) -> Result<HookInstallStatus, HookInstallError> {
        for file in self.files()? {
            file.remove()?;
        }
        Ok(self.status())
    }

    fn describe(&self, files: &[JsonHookTarget]) -> HookInstallStatus {
        if files.is_empty() {
            return self.status_with(
                HookInstallState::NotInstalled,
                Some(
                    HookInstallError::NoAgentFiles {
                        path: self.folder.to_string_lossy().into_owned(),
                    }
                    .to_string(),
                ),
            );
        }
        let states: Vec<HookInstallStatus> = files.iter().map(JsonHookTarget::status).collect();
        if let Some(broken) = states
            .iter()
            .find(|status| status.state == HookInstallState::Error)
        {
            return self.status_with(HookInstallState::Error, broken.detail.clone());
        }
        let installed = states
            .iter()
            .filter(|status| status.state == HookInstallState::Installed)
            .count();
        let state = if installed == states.len() {
            HookInstallState::Installed
        } else if installed == 0
            && states
                .iter()
                .all(|status| status.state == HookInstallState::NotInstalled)
        {
            HookInstallState::NotInstalled
        } else {
            HookInstallState::Partial
        };
        let detail = (state == HookInstallState::Partial)
            .then(|| "Some agent files do not have Lynx's hooks yet.".to_owned());
        self.status_with(state, detail)
    }

    fn status_with(&self, state: HookInstallState, detail: Option<String>) -> HookInstallStatus {
        HookInstallStatus {
            state,
            config_path: self.folder.to_string_lossy().into_owned(),
            events: EVENTS.iter().map(|spec| spec.name.to_owned()).collect(),
            detail,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXE: &str = "/opt/lynx/lynx-ide";

    fn agent_file(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, body).expect("write agent file");
        path
    }

    #[test]
    fn install_adds_the_hooks_to_every_agent_file_and_keeps_their_own() {
        let dir = tempfile::tempdir().expect("temp dir");
        let folder = dir.path().join("agents");
        fs::create_dir_all(&folder).expect("agents dir");
        agent_file(&folder, "a.json", r#"{"name":"a","tools":["read"]}"#);
        agent_file(
            &folder,
            "b.json",
            r#"{"name":"b","hooks":{"stop":[{"command":"other"}]}}"#,
        );
        let target = KiroHookTarget::new(folder.clone(), Some(Path::new(EXE)));
        assert_eq!(target.status().state, HookInstallState::NotInstalled);

        target.install().expect("install");

        assert_eq!(target.status().state, HookInstallState::Installed);
        let a: Value =
            serde_json::from_str(&fs::read_to_string(folder.join("a.json")).expect("read"))
                .expect("json");
        assert_eq!(a["name"], "a");
        assert_eq!(a["tools"][0], "read");
        let b: Value =
            serde_json::from_str(&fs::read_to_string(folder.join("b.json")).expect("read"))
                .expect("json");
        assert_eq!(b["hooks"]["stop"].as_array().map(Vec::len), Some(2));
    }

    #[test]
    fn a_new_agent_file_makes_the_status_partial() {
        let dir = tempfile::tempdir().expect("temp dir");
        let folder = dir.path().join("agents");
        fs::create_dir_all(&folder).expect("agents dir");
        agent_file(&folder, "a.json", "{}");
        let target = KiroHookTarget::new(folder.clone(), Some(Path::new(EXE)));
        target.install().expect("install");

        agent_file(&folder, "new.json", "{}");

        assert_eq!(target.status().state, HookInstallState::Partial);
    }

    #[test]
    fn install_refuses_a_folder_with_no_agent_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let target = KiroHookTarget::new(dir.path().join("agents"), Some(Path::new(EXE)));

        assert!(matches!(
            target.install(),
            Err(HookInstallError::NoAgentFiles { .. })
        ));
    }

    #[test]
    fn one_unreadable_agent_file_stops_the_whole_install() {
        let dir = tempfile::tempdir().expect("temp dir");
        let folder = dir.path().join("agents");
        fs::create_dir_all(&folder).expect("agents dir");
        agent_file(&folder, "a.json", "{}");
        agent_file(&folder, "b.json", "not json");
        let target = KiroHookTarget::new(folder.clone(), Some(Path::new(EXE)));

        assert!(target.install().is_err());
        assert_eq!(
            fs::read_to_string(folder.join("a.json")).expect("read"),
            "{}",
            "a readable file is not written when another one blocks the change"
        );
    }

    #[test]
    fn remove_takes_only_lynx_entries() {
        let dir = tempfile::tempdir().expect("temp dir");
        let folder = dir.path().join("agents");
        fs::create_dir_all(&folder).expect("agents dir");
        agent_file(&folder, "a.json", "{}");
        let target = KiroHookTarget::new(folder.clone(), Some(Path::new(EXE)));
        target.install().expect("install");

        target.remove().expect("remove");

        assert_eq!(target.status().state, HookInstallState::NotInstalled);
    }
}
