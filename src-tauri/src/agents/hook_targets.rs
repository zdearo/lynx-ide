//! Every harness's hook installer behind one state, so the settings screen asks for any harness the
//! same way. Each agent keeps its hooks in its own format, and this is where that format is picked.

use std::env;
use std::path::{Path, PathBuf};

use super::claude_hooks::{self, ClaudeHookTarget};
use super::codex_hooks::CodexHookTarget;
use super::harness::{HarnessId, executable_name, supported};
use super::hook_install::{HookInstallError, HookInstallState, HookInstallStatus, probe_version};
use super::mcp_install::McpTargets;
use super::omp_hooks;
use super::opencode_hooks;
use super::plugin_file::PluginFileTarget;

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
    /// The browser tools go in and out with the hooks, for the agents that take MCP servers.
    mcp: McpTargets,
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
        let mcp = McpTargets::new(
            folder("CLAUDE_CONFIG_DIR")
                .unwrap_or_else(|| user_home.to_path_buf())
                .join(".claude.json"),
            folder_or("CODEX_HOME", ".codex").join("config.toml"),
            opencode.join("opencode.json"),
            executable,
        );
        Self {
            mcp,
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
        if let Err(error) = self.mcp.install(harness) {
            status.detail = Some(format!("The browser tools were not added: {error}"));
        }
        Ok(status)
    }

    pub fn remove(&self, harness: HarnessId) -> Result<HookInstallStatus, HookInstallError> {
        let mut status = match self.target(harness) {
            HookTarget::Claude(target) => target.remove(probe_version(executable_name(harness))),
            HookTarget::Codex(target) => target.remove(probe_version(executable_name(harness))),
            HookTarget::Plugin(target) => target.remove(),
        }?;
        if let Err(error) = self.mcp.remove(harness) {
            status.detail = Some(format!("The browser tools were not removed: {error}"));
        }
        Ok(status)
    }

    /// For agents that were turned on before the browser tools existed: adds the tools where they
    /// are missing. Never rewrites an entry that is there. Blocks on the agent's CLI.
    pub fn add_missing_browser_tools(&self) {
        for harness in supported() {
            if !McpTargets::supports(harness)
                || self.status(harness).state != HookInstallState::Installed
            {
                continue;
            }
            if let Err(error) = self.mcp.install_if_missing(harness) {
                eprintln!("lynx: could not add the browser tools to {harness:?}: {error}");
            }
        }
    }
}
