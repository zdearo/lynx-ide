use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use specta::Type;

use crate::agents::harness::{HarnessId, harness_name, harness_named};

pub const DEFAULT_FONT_SIZE: u8 = 13;
pub const MIN_FONT_SIZE: u8 = 8;
pub const MAX_FONT_SIZE: u8 = 32;
pub const DEFAULT_SCROLLBACK: u32 = 5_000;
pub const MAX_SCROLLBACK: u32 = 100_000;
pub const MAX_LAUNCH_ARGUMENTS_CHARS: usize = 500;

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

/// The effective settings: what `~/.lynx/config.json` says, with defaults for the rest.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub terminal: TerminalConfig,
    pub worktrees: WorktreesConfig,
    pub agents: AgentsConfig,
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

    const HOME: &str = "/home/dev";

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
        for ok in ["/srv/trees", "~", "~/trees"] {
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
        assert_eq!(expand_home("~", home), PathBuf::from("/home/dev"));
        assert_eq!(
            expand_home("~/trees", home),
            PathBuf::from("/home/dev/trees")
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
}
