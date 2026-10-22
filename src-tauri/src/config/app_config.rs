use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Map, Value};
use specta::Type;

use crate::agents::harness::{HarnessId, harness_name, harness_named};
use crate::browser::AgentBrowserAccess;

pub const DEFAULT_FONT_SIZE: u8 = 13;
pub const MIN_FONT_SIZE: u8 = 8;
pub const MAX_FONT_SIZE: u8 = 32;
pub const DEFAULT_SCROLLBACK: u32 = 5_000;
pub const MAX_SCROLLBACK: u32 = 100_000;
pub const MAX_LAUNCH_ARGUMENTS_CHARS: usize = 500;
pub const MAX_COMMIT_INSTRUCTIONS_CHARS: usize = 4_000;
pub const MAX_ALLOWED_SITES: usize = 50;
pub const MAX_SITE_CHARS: usize = 253;

/// What a bell from a program does. Sound is left out: the terminal engine only reports the bell.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum BellStyle {
    None,
    #[default]
    Visual,
}

/// Themes the terminal can use. `Default` is the look the app has always had. A theme
/// gives the background; its text palette can also be used for the text.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ColorScheme {
    #[default]
    Default,
    Light,
    SolarizedDark,
    SolarizedLight,
    Dracula,
    Nord,
    CatppuccinLatte,
    CatppuccinFrappe,
    CatppuccinMacchiato,
    CatppuccinMocha,
    GruvboxDark,
    OneDark,
    TokyoNight,
    Monokai,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TerminalConfig {
    /// CSS font list tried before the bundled fonts; `None` uses the bundled ones.
    pub font_family: Option<String>,
    pub font_size: u8,
    /// Program for new terminals; `None` uses the user's login shell.
    pub shell: Option<String>,
    /// Lines of history kept per terminal.
    pub scrollback: u32,
    /// Copies a selection to the clipboard as soon as the mouse is released.
    pub copy_on_select: bool,
    /// Runs terminals in the PTY daemon, which keeps them when the app closes. Off, they end with it.
    pub pty_daemon: bool,
    pub bell: BellStyle,
    /// macOS only: the Option key sends Meta, which Emacs and readline shortcuts expect.
    pub mac_option_is_meta: bool,
    /// Closes the tab when its program exits, instead of keeping the last output.
    pub close_on_exit: bool,
    /// Asks before closing a tab whose process is alive, an idle shell included.
    pub confirm_close: bool,
    /// The theme: its background is the terminal's background.
    pub color_scheme: ColorScheme,
    /// Theme whose text palette (text colour and ANSI colours) the text uses; `None` uses `color_scheme`'s.
    pub text_color_scheme: Option<ColorScheme>,
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            font_family: None,
            font_size: DEFAULT_FONT_SIZE,
            shell: None,
            scrollback: DEFAULT_SCROLLBACK,
            copy_on_select: false,
            pty_daemon: false,
            bell: BellStyle::default(),
            mac_option_is_meta: false,
            close_on_exit: false,
            confirm_close: false,
            color_scheme: ColorScheme::default(),
            text_color_scheme: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorktreesConfig {
    /// Folder that holds new worktrees, grouped by repository; `None` puts them
    /// next to each repository. A leading `~` means the home folder.
    pub directory: Option<String>,
}

/// How every agent starts from a new terminal: `Manual` keeps its permission prompts, and `Yolo`
/// adds each agent's own flag that skips them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum LaunchMode {
    #[default]
    Manual,
    Yolo,
}

/// How agents are started from a new terminal.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentsConfig {
    pub launch_mode: LaunchMode,
    /// Arguments typed after an agent's command when a terminal starts it. An agent is in this map
    /// when its arguments are on; an empty text means on, with no arguments of its own.
    pub launch_arguments: BTreeMap<HarnessId, String>,
}

/// Which agent CLI writes a commit message. `Auto` takes the first one installed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum CommitMessageAgent {
    #[default]
    Auto,
    Claude,
    Codex,
    Opencode,
}

/// The style part of a commit message instruction. Lynx's output rules always follow it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum CommitMessageStyle {
    #[default]
    Conventional,
    Plain,
    Custom,
}

/// Git settings: which agent writes the commit message, and how it should write it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GitConfig {
    pub commit_message_agent: CommitMessageAgent,
    pub commit_message_style: CommitMessageStyle,
    /// The text used when the style is `Custom`. `None` means no text of its own.
    pub commit_message_instructions: Option<String>,
}

/// Orchestration: a coordinator agent starts workers for its tasks. Off until the user turns it on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OrchestrationConfig {
    pub enabled: bool,
    /// Close a worker's terminal once it has reported, so the sidebar keeps only live work.
    pub release_finished_workers: bool,
}

impl Default for OrchestrationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            release_finished_workers: true,
        }
    }
}

/// When Lynx sends a system notification, while the window is not in front.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct NotificationsConfig {
    /// An agent finished its work.
    pub on_done: bool,
    /// An agent is waiting for an answer or a permission.
    pub on_needs_you: bool,
    /// A new version of Lynx is out.
    pub on_update: bool,
}

impl Default for NotificationsConfig {
    fn default() -> Self {
        Self {
            on_done: true,
            on_needs_you: true,
            on_update: true,
        }
    }
}

/// How Lynx looks for new versions of itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdatesConfig {
    /// Look for a new version when Lynx starts and every few hours. Checking by hand always works.
    pub check_automatically: bool,
}

impl Default for UpdatesConfig {
    fn default() -> Self {
        Self {
            check_automatically: true,
        }
    }
}

/// What agents may do in Lynx's browser.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BrowserConfig {
    pub agent_access: AgentBrowserAccess,
    /// Sites agents may visit, each covering its subdomains. Empty means any site.
    pub allowed_sites: Vec<String>,
}

/// How the footer shows a plan's share: the percentage used, or what is left.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum UsageDisplay {
    #[default]
    Used,
    Remaining,
}

/// How much of each plan the footer shows: a percentage per plan, or every window with its bar.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum UsageLayout {
    #[default]
    Compact,
    Detailed,
}

pub const DEFAULT_USAGE_WARN_AT: u8 = 75;
pub const DEFAULT_USAGE_CRITICAL_AT: u8 = 90;
/// The plans whose limits Lynx reads. Settings offer these and no others.
pub const USAGE_PROVIDERS: [HarnessId; 2] = [HarnessId::Claude, HarnessId::Codex];

/// Plan limits in the footer: whether they show, which plans, how a share reads, and when a bar warns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UsageConfig {
    /// Off also stops Lynx from reading the limits at all.
    pub visible: bool,
    /// The plans to show, in order.
    pub providers: Vec<HarnessId>,
    /// Whether each plan's name shows beside its mark. Off leaves the mark, the bar and the number.
    pub show_names: bool,
    pub display: UsageDisplay,
    pub layout: UsageLayout,
    /// From this share used, a bar warns. Between 1 and 99.
    pub warn_at: u8,
    /// From this share used, a bar is nearly out. Between 2 and 100, and above `warn_at`.
    pub critical_at: u8,
}

impl Default for UsageConfig {
    fn default() -> Self {
        Self {
            visible: true,
            providers: USAGE_PROVIDERS.to_vec(),
            show_names: true,
            display: UsageDisplay::default(),
            layout: UsageLayout::default(),
            warn_at: DEFAULT_USAGE_WARN_AT,
            critical_at: DEFAULT_USAGE_CRITICAL_AT,
        }
    }
}

/// The effective settings: what `~/.lynx/config.json` says, with defaults for the rest.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub terminal: TerminalConfig,
    pub worktrees: WorktreesConfig,
    pub agents: AgentsConfig,
    pub git: GitConfig,
    pub orchestration: OrchestrationConfig,
    pub notifications: NotificationsConfig,
    pub browser: BrowserConfig,
    pub usage: UsageConfig,
    pub updates: UpdatesConfig,
}

/// One thing wrong with the config, addressed by its dotted key (`terminal.fontSize`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ConfigIssue {
    pub key: String,
    pub message: String,
}

impl ConfigIssue {
    fn new(key: &str, message: impl Into<String>) -> Self {
        Self {
            key: key.to_owned(),
            message: message.into(),
        }
    }
}

/// `~` and `~/rest` mean the home folder; anything else is taken as written.
pub fn expand_home(raw: &str, home: &Path) -> PathBuf {
    if raw == "~" {
        return home.to_owned();
    }
    match raw.strip_prefix("~/").or_else(|| raw.strip_prefix("~\\")) {
        Some(rest) => home.join(rest),
        None => PathBuf::from(raw),
    }
}

impl AppConfig {
    /// Trims text and turns blank text into "unset", so an emptied field means default.
    pub fn normalized(mut self) -> Self {
        for field in [
            &mut self.terminal.font_family,
            &mut self.terminal.shell,
            &mut self.worktrees.directory,
        ] {
            if let Some(text) = field.as_deref().map(str::trim) {
                *field = (!text.is_empty()).then(|| text.to_owned());
            }
        }
        for arguments in self.agents.launch_arguments.values_mut() {
            *arguments = arguments.trim().to_owned();
        }
        for site in &mut self.browser.allowed_sites {
            *site = site.trim().to_owned();
        }
        self.browser.allowed_sites.retain(|site| !site.is_empty());
        let instructions = &mut self.git.commit_message_instructions;
        if let Some(text) = instructions.as_deref().map(str::trim) {
            *instructions = (!text.is_empty()).then(|| text.to_owned());
        }
        self
    }

    pub fn validate(&self, home: &Path) -> Vec<ConfigIssue> {
        let mut issues = Vec::new();
        let terminal = &self.terminal;

        if let Some(family) = &terminal.font_family
            && family
                .chars()
                .any(|c| c.is_control() || matches!(c, ';' | '{' | '}'))
        {
            issues.push(ConfigIssue::new(
                "terminal.fontFamily",
                "use a comma-separated list of font names, like in CSS",
            ));
        }
        if !(MIN_FONT_SIZE..=MAX_FONT_SIZE).contains(&terminal.font_size) {
            issues.push(ConfigIssue::new(
                "terminal.fontSize",
                format!("must be between {MIN_FONT_SIZE} and {MAX_FONT_SIZE}"),
            ));
        }
        if let Some(shell) = &terminal.shell
            && shell.chars().any(char::is_control)
        {
            issues.push(ConfigIssue::new(
                "terminal.shell",
                "must be a single program path or name",
            ));
        }
        if terminal.scrollback > MAX_SCROLLBACK {
            issues.push(ConfigIssue::new(
                "terminal.scrollback",
                format!("must be at most {MAX_SCROLLBACK}"),
            ));
        }
        if let Some(directory) = &self.worktrees.directory
            && !expand_home(directory, home).is_absolute()
        {
            issues.push(ConfigIssue::new(
                "worktrees.directory",
                "must be an absolute path, or start with ~",
            ));
        }
        for (harness, arguments) in &self.agents.launch_arguments {
            let key = format!("agents.launchArguments.{}", harness_name(*harness));
            if arguments.chars().any(char::is_control) {
                issues.push(ConfigIssue::new(&key, "must be one line of arguments"));
            } else if arguments.chars().count() > MAX_LAUNCH_ARGUMENTS_CHARS {
                issues.push(ConfigIssue::new(
                    &key,
                    format!("must be at most {MAX_LAUNCH_ARGUMENTS_CHARS} characters"),
                ));
            }
        }
        let sites = &self.browser.allowed_sites;
        if sites.len() > MAX_ALLOWED_SITES {
            issues.push(ConfigIssue::new(
                "browser.allowedSites",
                format!("must list at most {MAX_ALLOWED_SITES} sites"),
            ));
        } else if let Some(site) = sites.iter().find(|site| !looks_like_site(site)) {
            issues.push(ConfigIssue::new(
                "browser.allowedSites",
                format!("{site} is not a site name; use names like example.com"),
            ));
        }
        let usage = &self.usage;
        if usage
            .providers
            .iter()
            .any(|harness| !USAGE_PROVIDERS.contains(harness))
        {
            issues.push(ConfigIssue::new(
                "usage.providers",
                "must list plan names such as claude and codex",
            ));
        }
        // A pair that contradicts itself is reset as a pair: resetting one alone would never fix it.
        if !(1..=99).contains(&usage.warn_at) {
            issues.push(ConfigIssue::new("usage.warnAt", "must be between 1 and 99"));
        } else if !(2..=100).contains(&usage.critical_at) {
            issues.push(ConfigIssue::new(
                "usage.criticalAt",
                "must be between 2 and 100",
            ));
        } else if usage.warn_at >= usage.critical_at {
            issues.push(ConfigIssue::new(
                "usage.thresholds",
                "getting close must be lower than almost used up",
            ));
        }
        if let Some(text) = &self.git.commit_message_instructions {
            if text.chars().any(|c| c.is_control() && c != '\n') {
                issues.push(ConfigIssue::new(
                    "git.commitMessageInstructions",
                    "must be plain text; line breaks are allowed",
                ));
            } else if text.chars().count() > MAX_COMMIT_INSTRUCTIONS_CHARS {
                issues.push(ConfigIssue::new(
                    "git.commitMessageInstructions",
                    format!("must be at most {MAX_COMMIT_INSTRUCTIONS_CHARS} characters"),
                ));
            }
        }
        issues
    }

    /// Puts one field back to its default, by the key an issue names.
    fn reset(&mut self, key: &str) {
        let defaults = AppConfig::default();
        match key {
            "terminal.fontFamily" => self.terminal.font_family = defaults.terminal.font_family,
            "terminal.fontSize" => self.terminal.font_size = defaults.terminal.font_size,
            "terminal.shell" => self.terminal.shell = defaults.terminal.shell,
            "terminal.scrollback" => self.terminal.scrollback = defaults.terminal.scrollback,
            "terminal.colorScheme" => self.terminal.color_scheme = defaults.terminal.color_scheme,
            "terminal.textColorScheme" => {
                self.terminal.text_color_scheme = defaults.terminal.text_color_scheme;
            }
            "terminal.copyOnSelect" => {
                self.terminal.copy_on_select = defaults.terminal.copy_on_select
            }
            "terminal.ptyDaemon" => self.terminal.pty_daemon = defaults.terminal.pty_daemon,
            "terminal.bell" => self.terminal.bell = defaults.terminal.bell,
            "terminal.macOptionIsMeta" => {
                self.terminal.mac_option_is_meta = defaults.terminal.mac_option_is_meta;
            }
            "terminal.closeOnExit" => self.terminal.close_on_exit = defaults.terminal.close_on_exit,
            "terminal.confirmClose" => {
                self.terminal.confirm_close = defaults.terminal.confirm_close
            }
            "worktrees.directory" => self.worktrees.directory = defaults.worktrees.directory,
            "agents.launchMode" => self.agents.launch_mode = defaults.agents.launch_mode,
            "notifications.onDone" => self.notifications.on_done = defaults.notifications.on_done,
            "notifications.onNeedsYou" => {
                self.notifications.on_needs_you = defaults.notifications.on_needs_you;
            }
            "notifications.onUpdate" => {
                self.notifications.on_update = defaults.notifications.on_update;
            }
            "updates.checkAutomatically" => {
                self.updates.check_automatically = defaults.updates.check_automatically;
            }
            "browser.agentAccess" => self.browser.agent_access = defaults.browser.agent_access,
            "browser.allowedSites" => self.browser.allowed_sites = defaults.browser.allowed_sites,
            "usage.visible" => self.usage.visible = defaults.usage.visible,
            "usage.providers" => self.usage.providers = defaults.usage.providers,
            "usage.showNames" => self.usage.show_names = defaults.usage.show_names,
            "usage.display" => self.usage.display = defaults.usage.display,
            "usage.layout" => self.usage.layout = defaults.usage.layout,
            "usage.warnAt" => self.usage.warn_at = defaults.usage.warn_at,
            "usage.criticalAt" => self.usage.critical_at = defaults.usage.critical_at,
            "usage.thresholds" => {
                self.usage.warn_at = defaults.usage.warn_at;
                self.usage.critical_at = defaults.usage.critical_at;
            }
            "git.commitMessageAgent" => {
                self.git.commit_message_agent = defaults.git.commit_message_agent;
            }
            "git.commitMessageStyle" => {
                self.git.commit_message_style = defaults.git.commit_message_style;
            }
            "git.commitMessageInstructions" => {
                self.git.commit_message_instructions = defaults.git.commit_message_instructions;
            }
            "orchestration.enabled" => self.orchestration.enabled = defaults.orchestration.enabled,
            "orchestration.releaseFinishedWorkers" => {
                self.orchestration.release_finished_workers =
                    defaults.orchestration.release_finished_workers;
            }
            key => {
                if let Some(harness) = key
                    .strip_prefix("agents.launchArguments.")
                    .and_then(harness_named)
                {
                    self.agents.launch_arguments.remove(&harness);
                }
            }
        }
    }
}

/// Reads the config leniently: a bad field is reported and replaced by its default
/// instead of failing the whole file. Unknown keys are ignored here (and kept on write).
pub fn parse_config(value: &Value, home: &Path) -> (AppConfig, Vec<ConfigIssue>) {
    let mut issues = Vec::new();
    let mut config = AppConfig::default();

    let Some(root) = value.as_object() else {
        issues.push(ConfigIssue::new("", "the top level must be a JSON object"));
        return (config, issues);
    };

    if let Some(terminal) = section(root, "terminal", &mut issues) {
        config.terminal.font_family =
            optional_text(terminal, "fontFamily", "terminal", &mut issues);
        config.terminal.shell = optional_text(terminal, "shell", "terminal", &mut issues);
        if let Some(size) = number(terminal, "fontSize", "terminal", &mut issues) {
            config.terminal.font_size = u8::try_from(size).unwrap_or(u8::MAX);
        }
        if let Some(lines) = number(terminal, "scrollback", "terminal", &mut issues) {
            config.terminal.scrollback = u32::try_from(lines).unwrap_or(u32::MAX);
        }
        if let Some(value) = flag(terminal, "copyOnSelect", "terminal", &mut issues) {
            config.terminal.copy_on_select = value;
        }
        if let Some(value) = flag(terminal, "ptyDaemon", "terminal", &mut issues) {
            config.terminal.pty_daemon = value;
        }
        if let Some(bell) = bell_style(terminal, &mut issues) {
            config.terminal.bell = bell;
        }
        if let Some(value) = flag(terminal, "macOptionIsMeta", "terminal", &mut issues) {
            config.terminal.mac_option_is_meta = value;
        }
        if let Some(value) = flag(terminal, "closeOnExit", "terminal", &mut issues) {
            config.terminal.close_on_exit = value;
        }
        if let Some(value) = flag(terminal, "confirmClose", "terminal", &mut issues) {
            config.terminal.confirm_close = value;
        }
        if let Some(scheme) = color_scheme(terminal, "colorScheme", &mut issues) {
            config.terminal.color_scheme = scheme;
        }
        if let Some(scheme) = color_scheme(terminal, "textColorScheme", &mut issues) {
            config.terminal.text_color_scheme = Some(scheme);
        }
    }
    if let Some(worktrees) = section(root, "worktrees", &mut issues) {
        config.worktrees.directory =
            optional_text(worktrees, "directory", "worktrees", &mut issues);
    }
    if let Some(agents) = section(root, "agents", &mut issues) {
        config.agents.launch_mode = launch_mode(agents, &mut issues).unwrap_or_default();
        config.agents.launch_arguments = launch_arguments(agents, &mut issues);
    }
    if let Some(notifications) = section(root, "notifications", &mut issues) {
        if let Some(value) = flag(notifications, "onDone", "notifications", &mut issues) {
            config.notifications.on_done = value;
        }
        if let Some(value) = flag(notifications, "onNeedsYou", "notifications", &mut issues) {
            config.notifications.on_needs_you = value;
        }
        if let Some(value) = flag(notifications, "onUpdate", "notifications", &mut issues) {
            config.notifications.on_update = value;
        }
    }
    if let Some(updates) = section(root, "updates", &mut issues)
        && let Some(value) = flag(updates, "checkAutomatically", "updates", &mut issues)
    {
        config.updates.check_automatically = value;
    }
    if let Some(browser) = section(root, "browser", &mut issues) {
        config.browser.agent_access =
            agent_browser_access(browser, &mut issues).unwrap_or_default();
        config.browser.allowed_sites = allowed_sites(browser, &mut issues);
    }
    if let Some(usage) = section(root, "usage", &mut issues) {
        if let Some(value) = flag(usage, "visible", "usage", &mut issues) {
            config.usage.visible = value;
        }
        if let Some(providers) = usage_providers(usage, &mut issues) {
            config.usage.providers = providers;
        }
        if let Some(value) = flag(usage, "showNames", "usage", &mut issues) {
            config.usage.show_names = value;
        }
        if let Some(display) = choice(usage, "display", "usage", "used or remaining", &mut issues) {
            config.usage.display = display;
        }
        if let Some(layout) = choice(usage, "layout", "usage", "compact or detailed", &mut issues) {
            config.usage.layout = layout;
        }
        if let Some(warn) = number(usage, "warnAt", "usage", &mut issues) {
            config.usage.warn_at = u8::try_from(warn).unwrap_or(u8::MAX);
        }
        if let Some(critical) = number(usage, "criticalAt", "usage", &mut issues) {
            config.usage.critical_at = u8::try_from(critical).unwrap_or(u8::MAX);
        }
    }
    if let Some(git) = section(root, "git", &mut issues) {
        config.git.commit_message_agent =
            commit_message_agent(git, &mut issues).unwrap_or_default();
        config.git.commit_message_style =
            commit_message_style(git, &mut issues).unwrap_or_default();
        config.git.commit_message_instructions =
            optional_text(git, "commitMessageInstructions", "git", &mut issues);
    }
    if let Some(orchestration) = section(root, "orchestration", &mut issues) {
        if let Some(value) = flag(orchestration, "enabled", "orchestration", &mut issues) {
            config.orchestration.enabled = value;
        }
        if let Some(value) = flag(
            orchestration,
            "releaseFinishedWorkers",
            "orchestration",
            &mut issues,
        ) {
            config.orchestration.release_finished_workers = value;
        }
    }

    finish(config.normalized(), issues, home)
}

fn launch_mode(agents: &Map<String, Value>, issues: &mut Vec<ConfigIssue>) -> Option<LaunchMode> {
    match optional_text(agents, "launchMode", "agents", issues)?.as_str() {
        "manual" => Some(LaunchMode::Manual),
        "yolo" => Some(LaunchMode::Yolo),
        _ => {
            issues.push(ConfigIssue::new(
                "agents.launchMode",
                "must be manual or yolo",
            ));
            None
        }
    }
}

fn agent_browser_access(
    browser: &Map<String, Value>,
    issues: &mut Vec<ConfigIssue>,
) -> Option<AgentBrowserAccess> {
    match optional_text(browser, "agentAccess", "browser", issues)?.as_str() {
        "off" => Some(AgentBrowserAccess::Off),
        "readOnly" => Some(AgentBrowserAccess::ReadOnly),
        "full" => Some(AgentBrowserAccess::Full),
        _ => {
            issues.push(ConfigIssue::new(
                "browser.agentAccess",
                "must be off, readOnly or full",
            ));
            None
        }
    }
}

fn allowed_sites(browser: &Map<String, Value>, issues: &mut Vec<ConfigIssue>) -> Vec<String> {
    let Some(value) = browser.get("allowedSites") else {
        return Vec::new();
    };
    let sites = value.as_array().and_then(|list| {
        list.iter()
            .map(|site| site.as_str().map(str::to_owned))
            .collect::<Option<Vec<_>>>()
    });
    match sites {
        Some(sites) => sites,
        None => {
            if !value.is_null() {
                issues.push(ConfigIssue::new(
                    "browser.allowedSites",
                    "must be a list of site names",
                ));
            }
            Vec::new()
        }
    }
}

/// A host name such as `example.com`, with an optional `*.` in front.
fn looks_like_site(site: &str) -> bool {
    let name = site.strip_prefix("*.").unwrap_or(site);
    !name.is_empty()
        && name.chars().count() <= MAX_SITE_CHARS
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'))
        && !name.starts_with(['.', '-'])
        && !name.ends_with(['.', '-'])
}

fn commit_message_agent(
    git: &Map<String, Value>,
    issues: &mut Vec<ConfigIssue>,
) -> Option<CommitMessageAgent> {
    match optional_text(git, "commitMessageAgent", "git", issues)?.as_str() {
        "auto" => Some(CommitMessageAgent::Auto),
        "claude" => Some(CommitMessageAgent::Claude),
        "codex" => Some(CommitMessageAgent::Codex),
        "opencode" => Some(CommitMessageAgent::Opencode),
        _ => {
            issues.push(ConfigIssue::new(
                "git.commitMessageAgent",
                "must be auto, claude, codex or opencode",
            ));
            None
        }
    }
}

fn commit_message_style(
    git: &Map<String, Value>,
    issues: &mut Vec<ConfigIssue>,
) -> Option<CommitMessageStyle> {
    match optional_text(git, "commitMessageStyle", "git", issues)?.as_str() {
        "conventional" => Some(CommitMessageStyle::Conventional),
        "plain" => Some(CommitMessageStyle::Plain),
        "custom" => Some(CommitMessageStyle::Custom),
        _ => {
            issues.push(ConfigIssue::new(
                "git.commitMessageStyle",
                "must be conventional, plain or custom",
            ));
            None
        }
    }
}

/// Reads each agent's arguments. Names Lynx does not know are left in the file and not read here.
fn launch_arguments(
    agents: &Map<String, Value>,
    issues: &mut Vec<ConfigIssue>,
) -> BTreeMap<HarnessId, String> {
    let mut arguments = BTreeMap::new();
    let Some(value) = agents.get("launchArguments") else {
        return arguments;
    };
    let Value::Object(by_agent) = value else {
        if !value.is_null() {
            issues.push(ConfigIssue::new(
                "agents.launchArguments",
                "must be an object, with an agent name for each entry",
            ));
        }
        return arguments;
    };
    for (name, value) in by_agent {
        let Some(harness) = harness_named(name) else {
            continue;
        };
        match value {
            Value::String(text) => {
                arguments.insert(harness, text.clone());
            }
            Value::Null => {}
            _ => issues.push(ConfigIssue::new(
                &format!("agents.launchArguments.{name}"),
                "must be text",
            )),
        }
    }
    arguments
}

/// Resets invalid fields one at a time until the config validates.
/// A text setting that must be one of the names its enum serializes to.
fn choice<T: DeserializeOwned>(
    object: &Map<String, Value>,
    key: &str,
    section: &str,
    expected: &str,
    issues: &mut Vec<ConfigIssue>,
) -> Option<T> {
    let text = optional_text(object, key, section, issues)?;
    match serde_json::from_value(Value::String(text)) {
        Ok(value) => Some(value),
        Err(_) => {
            issues.push(ConfigIssue::new(
                &format!("{section}.{key}"),
                format!("must be {expected}"),
            ));
            None
        }
    }
}

/// The plans to show: a list of plan names, each one Lynx reads, without repeats.
fn usage_providers(
    usage: &Map<String, Value>,
    issues: &mut Vec<ConfigIssue>,
) -> Option<Vec<HarnessId>> {
    let items = match usage.get("providers")? {
        Value::Array(items) => items,
        Value::Null => return None,
        _ => {
            issues.push(ConfigIssue::new(
                "usage.providers",
                "must be a list of plan names",
            ));
            return None;
        }
    };
    let mut providers: Vec<HarnessId> = Vec::new();
    for item in items {
        match serde_json::from_value::<HarnessId>(item.clone()) {
            Ok(harness) if !providers.contains(&harness) => providers.push(harness),
            Ok(_) => {}
            Err(_) => {
                issues.push(ConfigIssue::new(
                    "usage.providers",
                    "must list plan names such as claude and codex",
                ));
                return None;
            }
        }
    }
    Some(providers)
}

fn finish(
    mut config: AppConfig,
    mut issues: Vec<ConfigIssue>,
    home: &Path,
) -> (AppConfig, Vec<ConfigIssue>) {
    loop {
        let remaining = config.validate(home);
        let Some(issue) = remaining.into_iter().next() else {
            return (config, issues);
        };
        config.reset(&issue.key);
        issues.push(issue);
    }
}

fn section<'a>(
    root: &'a Map<String, Value>,
    name: &str,
    issues: &mut Vec<ConfigIssue>,
) -> Option<&'a Map<String, Value>> {
    match root.get(name)? {
        Value::Object(map) => Some(map),
        _ => {
            issues.push(ConfigIssue::new(name, "must be an object"));
            None
        }
    }
}

fn optional_text(
    object: &Map<String, Value>,
    key: &str,
    section: &str,
    issues: &mut Vec<ConfigIssue>,
) -> Option<String> {
    match object.get(key)? {
        Value::String(text) => Some(text.clone()),
        Value::Null => None,
        _ => {
            issues.push(ConfigIssue::new(
                &format!("{section}.{key}"),
                "must be text",
            ));
            None
        }
    }
}

fn flag(
    object: &Map<String, Value>,
    key: &str,
    section: &str,
    issues: &mut Vec<ConfigIssue>,
) -> Option<bool> {
    match object.get(key)? {
        Value::Bool(value) => Some(*value),
        Value::Null => None,
        _ => {
            issues.push(ConfigIssue::new(
                &format!("{section}.{key}"),
                "must be true or false",
            ));
            None
        }
    }
}

fn color_scheme(
    object: &Map<String, Value>,
    key: &str,
    issues: &mut Vec<ConfigIssue>,
) -> Option<ColorScheme> {
    let text = optional_text(object, key, "terminal", issues)?;
    // Same names as the serialized form, so the file and the interface agree.
    match serde_json::from_value(Value::String(text)) {
        Ok(scheme) => Some(scheme),
        Err(_) => {
            issues.push(ConfigIssue::new(
                &format!("terminal.{key}"),
                "must be one of the colour schemes the settings offer",
            ));
            None
        }
    }
}

fn bell_style(object: &Map<String, Value>, issues: &mut Vec<ConfigIssue>) -> Option<BellStyle> {
    match optional_text(object, "bell", "terminal", issues)?.as_str() {
        "none" => Some(BellStyle::None),
        "visual" => Some(BellStyle::Visual),
        _ => {
            issues.push(ConfigIssue::new("terminal.bell", "must be none or visual"));
            None
        }
    }
}

fn number(
    object: &Map<String, Value>,
    key: &str,
    section: &str,
    issues: &mut Vec<ConfigIssue>,
) -> Option<u64> {
    match object.get(key)? {
        Value::Number(number) => match number.as_u64() {
            Some(value) => Some(value),
            None => {
                issues.push(ConfigIssue::new(
                    &format!("{section}.{key}"),
                    "must be a whole number, zero or more",
                ));
                None
            }
        },
        Value::Null => None,
        _ => {
            issues.push(ConfigIssue::new(
                &format!("{section}.{key}"),
                "must be a number",
            ));
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    // Absolute on the platform the tests run on, so `~` expands to a real directory.
    const HOME: &str = if cfg!(windows) {
        r"C:\home\dev"
    } else {
        "/home/dev"
    };

    fn parse(value: Value) -> (AppConfig, Vec<ConfigIssue>) {
        parse_config(&value, Path::new(HOME))
    }

    fn keys(issues: &[ConfigIssue]) -> Vec<&str> {
        issues.iter().map(|issue| issue.key.as_str()).collect()
    }

    #[test]
    fn an_empty_object_means_all_defaults_and_no_complaints() {
        let (config, issues) = parse(json!({}));
        assert_eq!(config, AppConfig::default());
        assert!(issues.is_empty());
    }

    #[test]
    fn reads_every_known_field() {
        let (config, issues) = parse(json!({
            "terminal": { "fontFamily": "Fira Code", "fontSize": 16, "shell": "/usr/bin/fish", "scrollback": 20000 },
            "worktrees": { "directory": "~/trees" }
        }));
        assert!(issues.is_empty(), "{issues:?}");
        assert_eq!(config.terminal.font_family.as_deref(), Some("Fira Code"));
        assert_eq!(config.terminal.font_size, 16);
        assert_eq!(config.terminal.shell.as_deref(), Some("/usr/bin/fish"));
        assert_eq!(config.terminal.scrollback, 20000);
        assert_eq!(config.worktrees.directory.as_deref(), Some("~/trees"));
    }

    #[test]
    fn unknown_keys_are_ignored_without_complaint() {
        let (config, issues) =
            parse(json!({ "futureThing": 1, "terminal": { "newOption": true } }));
        assert_eq!(config, AppConfig::default());
        assert!(issues.is_empty());
    }

    #[test]
    fn a_bad_field_is_reported_and_defaulted_while_the_rest_still_applies() {
        let (config, issues) = parse(json!({
            "terminal": { "fontSize": 99, "scrollback": 1000, "shell": "/bin/zsh" }
        }));
        assert_eq!(keys(&issues), ["terminal.fontSize"]);
        assert_eq!(config.terminal.font_size, DEFAULT_FONT_SIZE);
        assert_eq!(config.terminal.scrollback, 1000);
        assert_eq!(config.terminal.shell.as_deref(), Some("/bin/zsh"));
    }

    #[test]
    fn wrong_types_are_reported_per_field() {
        let (config, issues) = parse(json!({
            "terminal": { "fontSize": "big", "shell": 5, "scrollback": -1, "fontFamily": ["a"] },
            "worktrees": { "directory": 7 }
        }));
        assert_eq!(
            keys(&issues),
            [
                "terminal.fontFamily",
                "terminal.shell",
                "terminal.fontSize",
                "terminal.scrollback",
                "worktrees.directory"
            ]
        );
        assert_eq!(config, AppConfig::default());
    }

    #[test]
    fn a_section_that_is_not_an_object_is_reported() {
        let (config, issues) = parse(json!({ "terminal": 3, "worktrees": [] }));
        assert_eq!(keys(&issues), ["terminal", "worktrees"]);
        assert_eq!(config, AppConfig::default());
    }

    #[test]
    fn a_top_level_that_is_not_an_object_is_reported() {
        let (config, issues) = parse(json!([1, 2]));
        assert_eq!(keys(&issues), [""]);
        assert_eq!(config, AppConfig::default());
    }

    #[test]
    fn several_invalid_values_are_all_reset_and_reported() {
        let (config, issues) = parse(json!({
            "terminal": { "fontSize": 1, "scrollback": 999999999 },
            "worktrees": { "directory": "relative/path" }
        }));
        assert_eq!(
            keys(&issues),
            [
                "terminal.fontSize",
                "terminal.scrollback",
                "worktrees.directory"
            ]
        );
        assert_eq!(config, AppConfig::default());
    }

    #[test]
    fn blank_text_means_unset() {
        let (config, issues) = parse(json!({ "terminal": { "shell": "   ", "fontFamily": "" } }));
        assert!(issues.is_empty());
        assert_eq!(config.terminal.shell, None);
        assert_eq!(config.terminal.font_family, None);
    }

    #[test]
    fn font_lists_cannot_smuggle_css() {
        let config = AppConfig {
            terminal: TerminalConfig {
                font_family: Some("Fira Code; color: red".to_owned()),
                ..TerminalConfig::default()
            },
            ..AppConfig::default()
        };
        assert_eq!(
            keys(&config.validate(Path::new(HOME))),
            ["terminal.fontFamily"]
        );
    }

    #[test]
    fn directories_accept_absolute_paths_and_a_home_prefix() {
        let absolute = if cfg!(windows) {
            r"C:\srv\trees"
        } else {
            "/srv/trees"
        };
        for ok in [absolute, "~", "~/trees"] {
            let config = AppConfig {
                worktrees: WorktreesConfig {
                    directory: Some(ok.to_owned()),
                },
                ..AppConfig::default()
            };
            assert!(config.validate(Path::new(HOME)).is_empty(), "{ok}");
        }
    }

    #[test]
    fn expands_the_home_prefix_only_at_the_start() {
        let home = Path::new(HOME);
        assert_eq!(expand_home("~", home), PathBuf::from(HOME));
        assert_eq!(
            expand_home("~/trees", home),
            PathBuf::from(HOME).join("trees")
        );
        assert_eq!(expand_home("/a/~/b", home), PathBuf::from("/a/~/b"));
        assert_eq!(expand_home("~user/x", home), PathBuf::from("~user/x"));
    }

    #[test]
    fn reads_the_terminal_behaviour_options() {
        let (config, issues) = parse(json!({
            "terminal": {
                "copyOnSelect": true,
                "bell": "none",
                "macOptionIsMeta": true,
                "closeOnExit": true,
                "confirmClose": true
            }
        }));
        assert!(issues.is_empty(), "{issues:?}");
        assert!(config.terminal.copy_on_select);
        assert_eq!(config.terminal.bell, BellStyle::None);
        assert!(config.terminal.mac_option_is_meta);
        assert!(config.terminal.close_on_exit);
        assert!(config.terminal.confirm_close);
    }

    #[test]
    fn behaviour_options_default_to_quiet_with_a_visual_bell() {
        let terminal = TerminalConfig::default();
        assert!(!terminal.copy_on_select);
        assert_eq!(terminal.bell, BellStyle::Visual);
        assert!(!terminal.mac_option_is_meta);
        assert!(!terminal.close_on_exit);
        assert!(!terminal.confirm_close);
    }

    #[test]
    fn a_bell_outside_the_list_and_non_boolean_flags_fall_back_to_defaults() {
        let (config, issues) = parse(json!({
            "terminal": { "bell": "sound", "copyOnSelect": "yes", "confirmClose": 0 }
        }));
        assert_eq!(
            keys(&issues),
            [
                "terminal.copyOnSelect",
                "terminal.bell",
                "terminal.confirmClose"
            ]
        );
        assert_eq!(config.terminal, TerminalConfig::default());
    }

    #[test]
    fn reads_the_theme_and_the_text_theme() {
        let (config, issues) = parse(json!({
            "terminal": { "colorScheme": "solarizedDark", "textColorScheme": "dracula" }
        }));
        assert!(issues.is_empty(), "{issues:?}");
        assert_eq!(config.terminal.color_scheme, ColorScheme::SolarizedDark);
        assert_eq!(
            config.terminal.text_color_scheme,
            Some(ColorScheme::Dracula)
        );
    }

    #[test]
    fn unknown_themes_fall_back_to_defaults() {
        let (config, issues) = parse(json!({
            "terminal": { "colorScheme": "neon", "textColorScheme": "red" }
        }));
        assert_eq!(
            keys(&issues),
            ["terminal.colorScheme", "terminal.textColorScheme"]
        );
        assert_eq!(config.terminal, TerminalConfig::default());
    }

    #[test]
    fn reads_the_arguments_of_each_agent_by_its_name() {
        let (config, issues) = parse(json!({
            "agents": { "launchArguments": { "claude": "  --model opus  " } }
        }));
        assert!(issues.is_empty());
        assert_eq!(
            config.agents.launch_arguments,
            BTreeMap::from([(HarnessId::Claude, "--model opus".to_owned())])
        );
    }

    #[test]
    fn an_empty_argument_text_means_on_with_no_arguments_of_its_own() {
        let (config, issues) = parse(json!({
            "agents": { "launchArguments": { "codex": "   " } }
        }));
        assert!(issues.is_empty());
        assert_eq!(
            config.agents.launch_arguments,
            BTreeMap::from([(HarnessId::Codex, String::new())])
        );
    }

    #[test]
    fn reads_the_launch_mode_and_manual_is_the_default() {
        assert_eq!(parse(json!({})).0.agents.launch_mode, LaunchMode::Manual);
        let (config, issues) = parse(json!({ "agents": { "launchMode": "yolo" } }));
        assert!(issues.is_empty());
        assert_eq!(config.agents.launch_mode, LaunchMode::Yolo);
    }

    #[test]
    fn an_unknown_launch_mode_is_reported_and_manual_is_used() {
        let (config, issues) = parse(json!({ "agents": { "launchMode": "fast" } }));
        assert_eq!(keys(&issues), ["agents.launchMode"]);
        assert_eq!(config.agents.launch_mode, LaunchMode::Manual);
    }

    #[test]
    fn unknown_agent_names_are_ignored_without_complaint() {
        let (config, issues) = parse(json!({
            "agents": { "launchArguments": { "mystery": "--x" } }
        }));
        assert!(issues.is_empty());
        assert!(config.agents.launch_arguments.is_empty());
    }

    #[test]
    fn arguments_must_be_text_on_one_line_and_not_too_long() {
        let (config, issues) = parse(json!({
            "agents": { "launchArguments": { "claude": 7, "codex": "--a\n--b" } }
        }));
        assert_eq!(
            keys(&issues),
            [
                "agents.launchArguments.claude",
                "agents.launchArguments.codex"
            ]
        );
        assert!(config.agents.launch_arguments.is_empty());

        let long = "x".repeat(MAX_LAUNCH_ARGUMENTS_CHARS + 1);
        let (_, issues) = parse(json!({
            "agents": { "launchArguments": { "claude": long } }
        }));
        assert_eq!(keys(&issues), ["agents.launchArguments.claude"]);
    }

    #[test]
    fn a_list_of_arguments_that_is_not_an_object_is_reported() {
        let (_, issues) = parse(json!({ "agents": { "launchArguments": ["--x"] } }));
        assert_eq!(keys(&issues), ["agents.launchArguments"]);
    }

    #[test]
    fn git_settings_default_to_automatic_agent_and_conventional_style() {
        let (config, issues) = parse(json!({}));

        assert!(issues.is_empty());
        assert_eq!(config.git.commit_message_agent, CommitMessageAgent::Auto);
        assert_eq!(
            config.git.commit_message_style,
            CommitMessageStyle::Conventional
        );
        assert_eq!(config.git.commit_message_instructions, None);
    }

    #[test]
    fn opencode_is_a_commit_message_agent_and_the_error_lists_all_of_them() {
        let (config, issues) = parse(json!({ "git": { "commitMessageAgent": "opencode" } }));
        assert!(issues.is_empty());
        assert_eq!(
            config.git.commit_message_agent,
            CommitMessageAgent::Opencode
        );

        let (_, issues) = parse(json!({ "git": { "commitMessageAgent": "gemini" } }));
        assert_eq!(keys(&issues), ["git.commitMessageAgent"]);
        assert!(issues[0].message.contains("opencode"));
    }

    #[test]
    fn reads_the_commit_message_agent_style_and_instructions() {
        let (config, issues) = parse(json!({
            "git": {
                "commitMessageAgent": "codex",
                "commitMessageStyle": "custom",
                "commitMessageInstructions": "Write in Portuguese."
            }
        }));

        assert!(issues.is_empty());
        assert_eq!(config.git.commit_message_agent, CommitMessageAgent::Codex);
        assert_eq!(config.git.commit_message_style, CommitMessageStyle::Custom);
        assert_eq!(
            config.git.commit_message_instructions.as_deref(),
            Some("Write in Portuguese.")
        );
    }

    #[test]
    fn an_unknown_agent_or_style_is_reported_and_defaulted() {
        let (config, issues) = parse(json!({
            "git": { "commitMessageAgent": "gemini", "commitMessageStyle": "emoji" }
        }));

        assert_eq!(
            keys(&issues),
            ["git.commitMessageAgent", "git.commitMessageStyle"]
        );
        assert_eq!(config.git.commit_message_agent, CommitMessageAgent::Auto);
        assert_eq!(
            config.git.commit_message_style,
            CommitMessageStyle::Conventional
        );
    }

    #[test]
    fn blank_instructions_mean_none_and_line_breaks_are_kept() {
        let (config, _) = parse(json!({ "git": { "commitMessageInstructions": "   " } }));
        assert_eq!(config.git.commit_message_instructions, None);

        let (config, issues) = parse(json!({
            "git": { "commitMessageInstructions": "First line.\nSecond line." }
        }));
        assert!(issues.is_empty());
        assert_eq!(
            config.git.commit_message_instructions.as_deref(),
            Some("First line.\nSecond line.")
        );
    }

    #[test]
    fn instructions_with_other_control_characters_or_too_long_are_reset() {
        let (config, issues) = parse(json!({
            "git": { "commitMessageInstructions": "bell\u{7}here" }
        }));
        assert_eq!(keys(&issues), ["git.commitMessageInstructions"]);
        assert_eq!(config.git.commit_message_instructions, None);

        let long = "x".repeat(MAX_COMMIT_INSTRUCTIONS_CHARS + 1);
        let (config, issues) = parse(json!({ "git": { "commitMessageInstructions": long } }));
        assert_eq!(keys(&issues), ["git.commitMessageInstructions"]);
        assert_eq!(config.git.commit_message_instructions, None);
    }

    #[test]
    fn notifications_default_to_on_and_read_each_flag() {
        let (config, issues) = parse(json!({}));
        assert!(issues.is_empty());
        assert!(config.notifications.on_done && config.notifications.on_needs_you);

        let (config, issues) = parse(json!({ "notifications": { "onDone": false } }));
        assert!(issues.is_empty());
        assert!(!config.notifications.on_done);
        assert!(config.notifications.on_needs_you);
    }

    #[test]
    fn an_invalid_notification_flag_is_reported_and_keeps_the_default() {
        let (config, issues) = parse(json!({ "notifications": { "onNeedsYou": "yes" } }));
        assert_eq!(keys(&issues), ["notifications.onNeedsYou"]);
        assert!(config.notifications.on_needs_you);
    }

    #[test]
    fn update_notifications_and_automatic_checks_default_to_on() {
        let (config, issues) = parse(json!({}));
        assert!(issues.is_empty());
        assert!(config.notifications.on_update);
        assert!(config.updates.check_automatically);

        let (config, issues) = parse(json!({
            "notifications": { "onUpdate": false },
            "updates": { "checkAutomatically": false },
        }));
        assert!(issues.is_empty());
        assert!(!config.notifications.on_update);
        assert!(!config.updates.check_automatically);
    }

    #[test]
    fn an_invalid_update_flag_is_reported_and_keeps_the_default() {
        let (config, issues) = parse(json!({ "updates": { "checkAutomatically": "no" } }));
        assert_eq!(keys(&issues), ["updates.checkAutomatically"]);
        assert!(config.updates.check_automatically);
    }

    #[test]
    fn the_browser_defaults_to_full_access_on_any_site() {
        let (config, issues) = parse(json!({}));
        assert!(issues.is_empty());
        assert_eq!(config.browser.agent_access, AgentBrowserAccess::Full);
        assert!(config.browser.allowed_sites.is_empty());
    }

    #[test]
    fn the_browser_settings_are_read_and_sites_are_trimmed() {
        let (config, issues) = parse(json!({
            "browser": { "agentAccess": "readOnly", "allowedSites": [" example.com ", "", "*.docs.rs"] }
        }));
        assert!(issues.is_empty());
        assert_eq!(config.browser.agent_access, AgentBrowserAccess::ReadOnly);
        assert_eq!(config.browser.allowed_sites, ["example.com", "*.docs.rs"]);
    }

    #[test]
    fn a_bad_browser_access_is_reported_and_keeps_the_default() {
        let (config, issues) = parse(json!({ "browser": { "agentAccess": "sometimes" } }));
        assert_eq!(keys(&issues), ["browser.agentAccess"]);
        assert_eq!(config.browser.agent_access, AgentBrowserAccess::Full);
    }

    #[test]
    fn a_site_list_that_is_not_names_is_reported_and_dropped() {
        let (config, issues) =
            parse(json!({ "browser": { "allowedSites": ["https://example.com/x"] } }));
        assert_eq!(keys(&issues), ["browser.allowedSites"]);
        assert!(config.browser.allowed_sites.is_empty());

        let (_, issues) = parse(json!({ "browser": { "allowedSites": "example.com" } }));
        assert_eq!(keys(&issues), ["browser.allowedSites"]);
    }

    #[test]
    fn usage_defaults_show_both_plans_compact_and_used() {
        let (config, issues) = parse(json!({}));

        assert!(issues.is_empty());
        assert!(config.usage.visible);
        assert_eq!(config.usage.providers, USAGE_PROVIDERS.to_vec());
        assert!(config.usage.show_names);
        assert_eq!(config.usage.display, UsageDisplay::Used);
        assert_eq!(config.usage.layout, UsageLayout::Compact);
        assert_eq!((config.usage.warn_at, config.usage.critical_at), (75, 90));
    }

    #[test]
    fn usage_values_are_read_from_the_file() {
        let (config, issues) = parse(json!({ "usage": {
            "visible": false,
            "providers": ["codex"],
            "showNames": false,
            "display": "remaining",
            "layout": "detailed",
            "warnAt": 60,
            "criticalAt": 85
        } }));

        assert!(issues.is_empty(), "{issues:?}");
        assert!(!config.usage.visible);
        assert_eq!(config.usage.providers, vec![HarnessId::Codex]);
        assert!(!config.usage.show_names);
        assert_eq!(config.usage.display, UsageDisplay::Remaining);
        assert_eq!(config.usage.layout, UsageLayout::Detailed);
        assert_eq!((config.usage.warn_at, config.usage.critical_at), (60, 85));
    }

    #[test]
    fn a_plan_lynx_cannot_read_is_reported_and_the_list_goes_back_to_default() {
        let (config, issues) = parse(json!({ "usage": { "providers": ["cursor"] } }));

        assert_eq!(keys(&issues), ["usage.providers"]);
        assert_eq!(config.usage.providers, USAGE_PROVIDERS.to_vec());
    }

    #[test]
    fn a_repeated_plan_is_listed_once() {
        let (config, issues) =
            parse(json!({ "usage": { "providers": ["codex", "claude", "codex"] } }));

        assert!(issues.is_empty());
        assert_eq!(
            config.usage.providers,
            vec![HarnessId::Codex, HarnessId::Claude]
        );
    }

    #[test]
    fn an_unknown_display_or_layout_falls_back_to_its_default() {
        let (config, issues) = parse(json!({ "usage": { "display": "half", "layout": "huge" } }));

        assert_eq!(keys(&issues), ["usage.display", "usage.layout"]);
        assert_eq!(config.usage.display, UsageDisplay::Used);
        assert_eq!(config.usage.layout, UsageLayout::Compact);
    }

    #[test]
    fn a_warn_level_out_of_range_is_reset_alone() {
        let (config, issues) = parse(json!({ "usage": { "warnAt": 150 } }));

        assert_eq!(keys(&issues), ["usage.warnAt"]);
        assert_eq!(config.usage.warn_at, DEFAULT_USAGE_WARN_AT);
        assert_eq!(config.usage.critical_at, DEFAULT_USAGE_CRITICAL_AT);
    }

    #[test]
    fn levels_that_contradict_each_other_are_reset_as_a_pair() {
        // A warning at 95 is above the default "almost out" at 90: neither setting alone can fix that.
        let (config, issues) = parse(json!({ "usage": { "warnAt": 95 } }));

        assert_eq!(keys(&issues), ["usage.thresholds"]);
        assert_eq!((config.usage.warn_at, config.usage.critical_at), (75, 90));
    }

    #[test]
    fn a_critical_level_below_the_warning_is_reset_as_a_pair() {
        let (config, issues) = parse(json!({ "usage": { "warnAt": 50, "criticalAt": 40 } }));

        assert_eq!(keys(&issues), ["usage.thresholds"]);
        assert_eq!((config.usage.warn_at, config.usage.critical_at), (75, 90));
    }

    #[test]
    fn orchestration_is_off_and_takes_the_defaults_until_the_user_sets_it() {
        let (config, issues) = parse(json!({}));
        assert!(issues.is_empty());
        assert_eq!(config.orchestration, OrchestrationConfig::default());
        assert!(!config.orchestration.enabled);
        assert!(config.orchestration.release_finished_workers);
    }

    #[test]
    fn reads_the_orchestration_settings() {
        let (config, issues) = parse(json!({
            "orchestration": { "enabled": true, "releaseFinishedWorkers": false }
        }));
        assert!(issues.is_empty());
        assert!(config.orchestration.enabled);
        assert!(!config.orchestration.release_finished_workers);
    }

    #[test]
    fn the_agent_worktree_and_model_are_chosen_per_worker_so_old_keys_are_not_read() {
        let (config, issues) = parse(json!({
            "orchestration": { "defaultAgent": "codex", "workerWorktree": "current" }
        }));
        assert!(issues.is_empty());
        assert_eq!(config.orchestration, OrchestrationConfig::default());
    }
}
