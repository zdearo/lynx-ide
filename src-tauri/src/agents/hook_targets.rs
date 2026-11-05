//! Every harness's hook installer behind one state, so the settings screen asks for any harness the
//! same way. Each agent keeps its hooks in its own format, and this is where that format is picked.

use std::env;
use std::path::{Path, PathBuf};

use super::claude_hooks::{self, ClaudeHookTarget};
use super::cli_permissions;
use super::codex_hooks::CodexHookTarget;
use super::harness::{HarnessId, executable_name, supported};
use super::hook_install::{HookInstallError, HookInstallState, HookInstallStatus, probe_version};
use super::mcp_cleanup::McpCleanup;
use super::omp_hooks;
use super::opencode_hooks;
use super::plugin_file::PluginFileTarget;
use super::skill_file;

/// One harness's place to write its hooks, by how the harness stores them.
#[derive(Clone)]
pub enum HookTarget {
    /// Claude's settings file. Its entries depend on the installed version.
    Claude(ClaudeHookTarget),
    /// Codex's hooks file and its trust list in `config.toml`.
    Codex(CodexHookTarget),
    /// A plugin file the agent loads from its own folder.
    Plugin(PluginFileTarget),
}

/// One field per harness, so every harness has a target and no lookup can miss.
#[derive(Clone)]
pub struct HookTargets {
    claude: HookTarget,
    codex: HookTarget,
    opencode: HookTarget,
    omp: HookTarget,
    /// The MCP entries of earlier builds, which the hooks' install and removal take out.
    mcp: McpCleanup,
    /// Where each agent reads its skills from. The `lynx` skill goes there with the hooks.
    skills: SkillFolders,
    /// The app's own executable, which the skill's commands run. None when it cannot be found.
    executable: Option<PathBuf>,
    /// Codex's home folder, whose `rules` folder holds the permission Lynx's CLI needs.
    codex_home: PathBuf,
    /// OpenCode's `opencode.json`, which holds the permission Lynx's CLI needs.
    opencode_config: PathBuf,
}

/// One folder per harness for the skills it reads.
#[derive(Clone)]
struct SkillFolders {
    claude: PathBuf,
    codex: PathBuf,
    opencode: PathBuf,
    omp: PathBuf,
}

impl SkillFolders {
    fn of(&self, harness: HarnessId) -> &Path {
        match harness {
            HarnessId::Claude => &self.claude,
            HarnessId::Codex => &self.codex,
            HarnessId::Opencode => &self.opencode,
            HarnessId::Omp => &self.omp,
        }
    }
}

impl HookTargets {
    /// Where each agent keeps its settings: the folder its own variable names, else its default.
    pub fn from_environment(user_home: &Path, executable: Option<&Path>) -> Self {
        let folder = |name: &str| env::var_os(name).map(PathBuf::from);
        let folder_or =
            |name: &str, default: &str| folder(name).unwrap_or_else(|| user_home.join(default));
        let opencode = folder("OPENCODE_CONFIG_DIR")
            .or_else(|| folder("XDG_CONFIG_HOME").map(|base| base.join("opencode")))
            .unwrap_or_else(|| user_home.join(".config").join("opencode"));
        let mcp = McpCleanup::new(
            folder("CLAUDE_CONFIG_DIR")
                .unwrap_or_else(|| user_home.to_path_buf())
                .join(".claude.json"),
            folder_or("CODEX_HOME", ".codex").join("config.toml"),
            opencode.join("opencode.json"),
        );
        let skills = SkillFolders {
            claude: folder("CLAUDE_CONFIG_DIR")
                .unwrap_or_else(|| user_home.join(".claude"))
                .join("skills"),
            codex: user_home.join(".agents").join("skills"),
            opencode: opencode.join("skills"),
            omp: user_home.join(".omp").join("agent").join("skills"),
        };
        Self {
            mcp,
            skills,
            executable: executable.map(Path::to_path_buf),
            codex_home: folder_or("CODEX_HOME", ".codex"),
            opencode_config: opencode.join("opencode.json"),
            claude: HookTarget::Claude(ClaudeHookTarget::new(
                claude_hooks::settings_path(folder("CLAUDE_CONFIG_DIR"), user_home),
                executable,
            )),
            codex: HookTarget::Codex(CodexHookTarget::new(
                folder_or("CODEX_HOME", ".codex"),
                executable,
            )),
            opencode: HookTarget::Plugin(opencode_hooks::target(opencode, executable)),
            omp: HookTarget::Plugin(omp_hooks::target(
                user_home.join(".omp").join("agent"),
                executable,
            )),
        }
    }

    fn target(&self, harness: HarnessId) -> &HookTarget {
        match harness {
            HarnessId::Claude => &self.claude,
            HarnessId::Codex => &self.codex,
            HarnessId::Opencode => &self.opencode,
            HarnessId::Omp => &self.omp,
        }
    }

    pub fn status(&self, harness: HarnessId) -> HookInstallStatus {
        match self.target(harness) {
            HookTarget::Claude(target) => target.status(probe_version(executable_name(harness))),
            HookTarget::Codex(target) => target.status(probe_version(executable_name(harness))),
            HookTarget::Plugin(target) => target.status(),
        }
    }

    /// Turns the agent on: hooks first, then the browser tools. The hooks are what makes the
    /// agent "on", so a refused MCP change leaves them in and says so in `detail`.
    pub fn install(&self, harness: HarnessId) -> Result<HookInstallStatus, HookInstallError> {
        let mut status = match self.target(harness) {
            HookTarget::Claude(target) => target.install(probe_version(executable_name(harness))),
            HookTarget::Codex(target) => target.install(probe_version(executable_name(harness))),
            HookTarget::Plugin(target) => target.install(),
        }?;
        // Entries from earlier builds go; the tools come through the skill now.
        if let Err(error) = self.mcp.remove_if_present(harness) {
            status.detail = Some(format!("The old MCP entry was not removed: {error}"));
        }
        if let Err(error) = self.write_skill(harness) {
            status.detail = Some(format!("The lynx skill was not added: {error}"));
        }
        if let Err(error) = self.write_cli_permission(harness, self.executable.as_deref()) {
            status.detail = Some(format!("The CLI permission was not added: {error}"));
        }
        Ok(status)
    }

    /// Lets the agent run the app's CLI without asking, for the agents whose permissions Lynx writes. Claude's
    /// rule is in its own settings, with its hooks. `None` takes Lynx's rule out again.
    fn write_cli_permission(
        &self,
        harness: HarnessId,
        executable: Option<&Path>,
    ) -> Result<(), HookInstallError> {
        match harness {
            HarnessId::Codex => cli_permissions::write_codex_rule(&self.codex_home, executable),
            HarnessId::Opencode => {
                cli_permissions::write_opencode_rule(&self.opencode_config, executable)
            }
            HarnessId::Claude | HarnessId::Omp => Ok(()),
        }
    }

    /// Writes the `lynx` skill for the agent, when the app's executable is known.
    fn write_skill(&self, harness: HarnessId) -> Result<(), String> {
        let Some(executable) = self.executable.as_deref() else {
            return Err("the app's executable could not be found".to_owned());
        };
        skill_file::write(
            self.skills.of(harness),
            &skill_file::agent_skill(executable),
        )
        .map_err(|error| error.to_string())
    }

    pub fn remove(&self, harness: HarnessId) -> Result<HookInstallStatus, HookInstallError> {
        let mut status = match self.target(harness) {
            HookTarget::Claude(target) => target.remove(probe_version(executable_name(harness))),
            HookTarget::Codex(target) => target.remove(probe_version(executable_name(harness))),
            HookTarget::Plugin(target) => target.remove(),
        }?;
        if let Err(error) = self.mcp.remove_if_present(harness) {
            status.detail = Some(format!("The old MCP entry was not removed: {error}"));
        }
        if let Err(error) = skill_file::remove(self.skills.of(harness)) {
            status.detail = Some(format!("The lynx skill was not removed: {error}"));
        }
        if let Err(error) = self.write_cli_permission(harness, None) {
            status.detail = Some(format!("The CLI permission was not removed: {error}"));
        }
        Ok(status)
    }

    /// For the agents that are on: rewrites the `lynx` skill so it names this app and the current tools, and
    /// takes out an MCP entry an earlier build left. Blocks on the agent's CLI only when there is an entry.
    pub fn refresh_agent_tools(&self) {
        for harness in supported() {
            if self.status(harness).state != HookInstallState::Installed {
                continue;
            }
            if let Err(error) = self.mcp.remove_if_present(harness) {
                eprintln!("lynx: could not remove the old MCP entry of {harness:?}: {error}");
            }
            if let Err(error) = self.write_skill(harness) {
                eprintln!("lynx: could not write the lynx skill for {harness:?}: {error}");
            }
            if let Err(error) = self.write_cli_permission(harness, self.executable.as_deref()) {
                eprintln!("lynx: could not write the CLI permission for {harness:?}: {error}");
            }
        }
    }
}
