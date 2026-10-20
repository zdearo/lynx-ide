//! An AppImage points the process at its own libraries and data; programs Lynx starts for the user must not inherit that.

use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use portable_pty::CommandBuilder;

/// Set by the AppImage runtime itself, never meaningful to a child.
const RUNTIME_VARIABLES: [&str; 4] = ["APPDIR", "APPIMAGE", "OWD", "ARGV0"];

/// What a child process must change in the environment it would otherwise inherit.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct EnvironmentCleanup {
    removed: Vec<OsString>,
    replaced: Vec<(OsString, OsString)>,
}

impl EnvironmentCleanup {
    /// Empty unless the variables say this process runs from a mounted AppImage.
    pub fn from_variables(variables: impl IntoIterator<Item = (OsString, OsString)>) -> Self {
        let variables: Vec<(OsString, OsString)> = variables.into_iter().collect();
        let value_of = |name: &str| {
            variables
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value)
        };
        let (Some(app_dir), Some(_)) = (value_of("APPDIR"), value_of("APPIMAGE")) else {
            return Self::default();
        };
        if app_dir.is_empty() {
            return Self::default();
        }
        let app_dir = Path::new(app_dir);

        let mut cleanup = Self::default();
        for (key, value) in &variables {
            if RUNTIME_VARIABLES.iter().any(|name| key == name) {
                cleanup.removed.push(key.clone());
                continue;
            }
            let entries: Vec<PathBuf> = env::split_paths(value).collect();
            if !entries.iter().any(|entry| entry.starts_with(app_dir)) {
                continue;
            }
            let kept: Vec<PathBuf> = entries
                .into_iter()
                .filter(|entry| !entry.as_os_str().is_empty() && !entry.starts_with(app_dir))
                .collect();
            match env::join_paths(kept) {
                Ok(joined) if !joined.is_empty() => {
                    cleanup.replaced.push((key.clone(), joined));
                }
                _ => cleanup.removed.push(key.clone()),
            }
        }
        cleanup
    }

    pub fn apply_to_command(&self, command: &mut Command) {
        for key in &self.removed {
            command.env_remove(key);
        }
        for (key, value) in &self.replaced {
            command.env(key, value);
        }
    }

    pub fn apply_to_terminal(&self, command: &mut CommandBuilder) {
        for key in &self.removed {
            command.env_remove(key);
        }
        for (key, value) in &self.replaced {
            command.env(key, value);
        }
    }
}

/// The cleanup for this process, read once: the environment of a running Lynx does not change.
fn current() -> &'static EnvironmentCleanup {
    static CURRENT: OnceLock<EnvironmentCleanup> = OnceLock::new();
    CURRENT.get_or_init(|| EnvironmentCleanup::from_variables(env::vars_os()))
}

/// For any program Lynx runs on the user's behalf (git, gh, agent CLIs).
pub fn clean_command(command: &mut Command) {
    current().apply_to_command(command);
}

/// For shells in terminals, before the variables a session asks for are set.
pub fn clean_terminal(command: &mut CommandBuilder) {
    current().apply_to_terminal(command);
}

/// The AppImage file itself: its mount disappears when Lynx closes, and the file runs without the mount's environment.
pub fn launch_executable(fallback: Option<PathBuf>) -> Option<PathBuf> {
    launch_executable_from(env::var_os("APPIMAGE"), fallback)
}

fn launch_executable_from(
    app_image: Option<OsString>,
    fallback: Option<PathBuf>,
) -> Option<PathBuf> {
    app_image
        .filter(|path| !path.is_empty() && Path::new(path).is_file())
        .map(PathBuf::from)
        .or(fallback)
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::*;

    fn variables(pairs: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
        pairs
            .iter()
            .map(|(key, value)| (OsString::from(key), OsString::from(value)))
            .collect()
    }

    fn inside_app_image() -> Vec<(OsString, OsString)> {
        variables(&[
            ("APPDIR", "/tmp/.mount_lynx.aPdmAME"),
            ("APPIMAGE", "/home/me/AppImages/lynx.appimage"),
            ("OWD", "/home/me"),
            ("PYTHONHOME", "/tmp/.mount_lynx.aPdmAME/usr/"),
            (
                "GDK_PIXBUF_MODULE_FILE",
                "/tmp/.mount_lynx.aPdmAME//usr/lib/gdk-pixbuf-2.0/2.10.0/loaders.cache",
            ),
            (
                "GST_PLUGIN_SYSTEM_PATH",
                "/tmp/.mount_lynx.aPdmAME/usr/lib/gstreamer:",
            ),
            (
                "LD_LIBRARY_PATH",
                "/tmp/.mount_lynx.aPdmAME/usr/lib/:/opt/mine/lib:/tmp/.mount_lynx.aPdmAME/lib/",
            ),
            (
                "PATH",
                "/home/me/bin:/tmp/.mount_lynx.aPdmAME/usr/bin/:/usr/bin:/tmp/.mount_lynx.aPdmAME-other/bin",
            ),
            ("HOME", "/home/me"),
        ])
    }

    fn replaced<'a>(cleanup: &'a EnvironmentCleanup, key: &str) -> Option<&'a OsString> {
        cleanup
            .replaced
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value)
    }

    #[test]
    fn outside_an_app_image_nothing_changes() {
        let without_image = EnvironmentCleanup::from_variables(variables(&[
            ("APPDIR", "/tmp/.mount_lynx.aPdmAME"),
            ("PATH", "/tmp/.mount_lynx.aPdmAME/usr/bin:/usr/bin"),
        ]));
        assert_eq!(without_image, EnvironmentCleanup::default());

        let without_directory =
            EnvironmentCleanup::from_variables(variables(&[("PATH", "/usr/bin")]));
        assert_eq!(without_directory, EnvironmentCleanup::default());
    }

    #[test]
    fn removes_the_runtime_variables() {
        let cleanup = EnvironmentCleanup::from_variables(inside_app_image());
        for name in ["APPDIR", "APPIMAGE", "OWD"] {
            assert!(cleanup.removed.iter().any(|key| key == name), "{name}");
        }
    }

    #[test]
    fn removes_variables_that_only_point_into_the_app_image() {
        let cleanup = EnvironmentCleanup::from_variables(inside_app_image());
        for name in [
            "PYTHONHOME",
            "GDK_PIXBUF_MODULE_FILE",
            "GST_PLUGIN_SYSTEM_PATH",
        ] {
            assert!(cleanup.removed.iter().any(|key| key == name), "{name}");
        }
    }

    #[test]
    fn keeps_the_users_own_entries_in_order() {
        let cleanup = EnvironmentCleanup::from_variables(inside_app_image());
        assert_eq!(
            replaced(&cleanup, "LD_LIBRARY_PATH"),
            Some(&OsString::from("/opt/mine/lib"))
        );
        // A sibling directory with the same prefix is not inside the mount.
        assert_eq!(
            replaced(&cleanup, "PATH"),
            Some(&OsString::from(
                "/home/me/bin:/usr/bin:/tmp/.mount_lynx.aPdmAME-other/bin"
            ))
        );
    }

    #[test]
    fn leaves_unrelated_variables_alone() {
        let cleanup = EnvironmentCleanup::from_variables(inside_app_image());
        assert!(!cleanup.removed.iter().any(|key| key == "HOME"));
        assert!(replaced(&cleanup, "HOME").is_none());
    }

    #[test]
    fn applies_to_a_command() {
        let cleanup = EnvironmentCleanup::from_variables(inside_app_image());
        let mut command = Command::new("true");
        cleanup.apply_to_command(&mut command);
        let envs: Vec<_> = command.get_envs().collect();
        assert!(envs.contains(&(OsStr::new("APPDIR"), None)));
        assert!(envs.contains(&(
            OsStr::new("LD_LIBRARY_PATH"),
            Some(OsStr::new("/opt/mine/lib"))
        )));
    }

    #[test]
    fn applies_to_a_terminal() {
        let cleanup = EnvironmentCleanup::from_variables(inside_app_image());
        let mut command = CommandBuilder::new("sh");
        command.env("PYTHONHOME", "/tmp/.mount_lynx.aPdmAME/usr/");
        command.env(
            "LD_LIBRARY_PATH",
            "/tmp/.mount_lynx.aPdmAME/usr/lib/:/opt/mine/lib",
        );
        cleanup.apply_to_terminal(&mut command);
        assert_eq!(command.get_env("PYTHONHOME"), None);
        assert_eq!(
            command.get_env("LD_LIBRARY_PATH"),
            Some(OsStr::new("/opt/mine/lib"))
        );
    }

    #[test]
    fn launches_the_app_image_file_when_it_exists() {
        let folder = tempfile::tempdir().unwrap();
        let file = folder.path().join("lynx.appimage");
        std::fs::write(&file, b"").unwrap();
        let fallback = Some(PathBuf::from("/tmp/.mount_lynx.aPdmAME/usr/bin/lynx-ide"));

        assert_eq!(
            launch_executable_from(Some(file.clone().into_os_string()), fallback.clone()),
            Some(file)
        );
        let gone = folder.path().join("gone").into_os_string();
        assert_eq!(
            launch_executable_from(Some(gone), fallback.clone()),
            fallback
        );
        assert_eq!(launch_executable_from(None, fallback.clone()), fallback);
    }
}
