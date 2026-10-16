/// The program a terminal starts, and its arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shell {
    pub program: String,
    pub args: Vec<String>,
}

impl Shell {
    /// The user's own shell. On Unix it runs as a login shell so a desktop-launched
    /// app still picks up the PATH the user set in their profile.
    #[cfg(unix)]
    pub fn system_default() -> Self {
        let program = std::env::var("SHELL")
            .ok()
            .filter(|shell| !shell.is_empty() && std::path::Path::new(shell).is_file())
            .unwrap_or_else(|| "/bin/sh".to_owned());
        Self {
            program,
            args: vec!["-l".to_owned()],
        }
    }

    #[cfg(windows)]
    pub fn system_default() -> Self {
        Self {
            program: std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_owned()),
            args: Vec::new(),
        }
    }

    /// The program the user configured, or their own shell when none is set.
    pub fn from_config(program: Option<&str>) -> Self {
        match program {
            Some(program) => Self {
                program: program.to_owned(),
                args: Self::system_default().args,
            },
            None => Self::system_default(),
        }
    }

    /// Short name for tab titles, e.g. `zsh`.
    pub fn display_name(&self) -> String {
        std::path::Path::new(&self.program).file_stem().map_or_else(
            || self.program.clone(),
            |name| name.to_string_lossy().into_owned(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_name_drops_directory_and_extension() {
        let shell = Shell {
            program: "/usr/bin/zsh".to_owned(),
            args: Vec::new(),
        };
        assert_eq!(shell.display_name(), "zsh");
    }

    #[cfg(unix)]
    #[test]
    fn default_shell_is_a_login_shell_that_exists() {
        let shell = Shell::system_default();
        assert!(std::path::Path::new(&shell.program).is_file());
        assert_eq!(shell.args, ["-l"]);
    }
}
