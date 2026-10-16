//! A plugin file an agent loads from its own folder when it starts. Lynx writes one file there and
//! owns it by a marker on its first line, so removal never touches a plugin someone else wrote.

use std::fs;
use std::path::PathBuf;

use super::hook_install::{
    HookInstallError, HookInstallState, HookInstallStatus, invalid_config, io_error, read_optional,
    write_atomic,
};

#[derive(Clone)]
pub struct PluginFileTarget {
    file: PathBuf,
    /// The first line of every file Lynx writes for this agent.
    marker: String,
    /// The file for this build, or None when the executable cannot be found.
    source: Option<String>,
    /// The events the plugin forwards, so the confirmation can name them.
    events: &'static [&'static str],
}

impl PluginFileTarget {
    pub fn new(
        file: PathBuf,
        marker: String,
        source: Option<String>,
        events: &'static [&'static str],
    ) -> Self {
        Self {
            file,
            marker,
            source,
            events,
        }
    }

    pub fn status(&self) -> HookInstallStatus {
        let Some(source) = self.source.as_deref() else {
            return self.status_with(
                HookInstallState::Error,
                Some(HookInstallError::NoExecutable.to_string()),
            );
        };
        match read_optional(&self.file) {
            Ok(None) => self.status_with(HookInstallState::NotInstalled, None),
            Ok(Some(text)) if text == source => self.status_with(HookInstallState::Installed, None),
            Ok(Some(text)) if text.starts_with(&self.marker) => self.status_with(
                HookInstallState::Partial,
                Some(
                    "The plugin points at another Lynx build; update the connection to fix it."
                        .to_owned(),
                ),
            ),
            // Not ours: leave the file alone and say so.
            Ok(Some(_)) => self.status_with(
                HookInstallState::Error,
                Some(invalid_config(&self.file).to_string()),
            ),
            Err(error) => self.status_with(HookInstallState::Error, Some(error.to_string())),
        }
    }

    /// Writes the plugin. A file with another name is never touched, and one with another
    /// owner's content at this name is refused.
    pub fn install(&self) -> Result<HookInstallStatus, HookInstallError> {
        let source = self
            .source
            .as_deref()
            .ok_or(HookInstallError::NoExecutable)?;
        if let Some(text) = read_optional(&self.file)?
            && !text.starts_with(&self.marker)
        {
            return Err(invalid_config(&self.file));
        }
        write_atomic(&self.file, source)?;
        Ok(self.status())
    }

    /// Deletes the plugin, if the file is Lynx's.
    pub fn remove(&self) -> Result<HookInstallStatus, HookInstallError> {
        if let Some(text) = read_optional(&self.file)?
            && text.starts_with(&self.marker)
        {
            fs::remove_file(&self.file).map_err(|source| io_error(&self.file, source))?;
        }
        Ok(self.status())
    }

    fn status_with(&self, state: HookInstallState, detail: Option<String>) -> HookInstallStatus {
        HookInstallStatus {
            state,
            config_path: self.file.to_string_lossy().into_owned(),
            events: self
                .events
                .iter()
                .map(|event| (*event).to_owned())
                .collect(),
            detail,
        }
    }
}
