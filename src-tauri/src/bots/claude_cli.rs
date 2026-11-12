//! Claude through its own CLI, as a bot's engine. The user signs in to the CLI as usual. Lynx starts
//! the unmodified binary in the bot's folder, with the bot's instructions and no tools, and never
//! reads its credentials.

use std::collections::HashMap;
use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use specta::Type;

use crate::agents::skill_file;
use crate::agents::tool_catalog::list_for;
use crate::orchestration::tools as orchestration_tools;

use super::base_instructions;
use super::bot_bridge::{BOT_SESSION_PREFIX, BotBridge, session_of};
use super::bot_folder::{BotError, BotFolder};
use super::bot_mcp;
use super::bot_memory;
use super::bot_skills;
use super::bot_tools;
use super::bot_topics;
use super::command_permissions;
use crate::private_file;
use crate::process::app_image_environment::clean_command;
use crate::process::bounded_command::{BoundedError, run_streaming};

/// How long one answer may take before Lynx gives up on it.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(120);
/// Where Lynx keeps the files it hands the CLI, inside the bot's folder.
const RUN_DIR: &str = ".run";
/// The built-in tools a bot may use when it reads a granted repository. Nothing that writes or runs.
const READ_TOOLS: [&str; 3] = ["Read", "Glob", "Grep"];
/// The tool that runs a skill. A bot with skills gets it and nothing else that runs.
const SKILL_TOOL: &str = "Skill";
/// The tool that runs commands. A bot gets it only with patterns to run: the CLI refuses any command
/// that does not match one, and one chained to a match.
const BASH_TOOL: &str = "Bash";
/// Files a bot is never let read, even inside a granted repository: keys, tokens and credentials.
const SECRET_DENIALS: [&str; 6] = [
    "Read(**/.env)",
    "Read(**/.env.*)",
    "Read(**/*.pem)",
    "Read(**/*.key)",
    "Read(**/id_rsa*)",
    "Read(**/credentials*)",
];

/// What a question came back as: the reply, or the reason there is none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeAnswer {
    pub ok: bool,
    pub text: String,
    /// The person stopped the answer: `text` is what had been written until then.
    pub stopped: bool,
}

#[derive(Debug, thiserror::Error)]
enum ClaudeCliError {
    #[error("the Claude CLI is not installed")]
    NotInstalled,
    #[error("Claude did not answer in time")]
    TimedOut,
    #[error("could not run the Claude CLI: {0}")]
    Io(#[from] io::Error),
    #[error("{0}")]
    Bot(#[from] BotError),
}

/// Runs the Claude CLI for a bot. Each bot's CLI runs in that bot's own folder.
pub struct ClaudeCli {
    bots_dir: PathBuf,
    home: PathBuf,
    /// How a bot's CLI reaches Lynx. Set once the app's hub is up; before that, bots have no tools.
    bridge: OnceLock<BotBridge>,
    /// The turns being written, by session, each with the flag that stops it.
    running: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

/// What a run may reach beyond its prompt: the MCP servers it starts, the folders it may read, and
/// whether it loads the bot's skills.
#[derive(Default)]
struct Reach {
    /// The bot's own servers, and Lynx's server when the hub has an address.
    servers: Map<String, Value>,
    read_dirs: Vec<PathBuf>,
    skills: bool,
    /// The command patterns the bot may run.
    commands: Vec<String>,
    /// Set to stop the answer being written.
    cancel: Arc<AtomicBool>,
    /// What the turn is called in the bot's `.run/` folder, so turns that overlap never share files.
    label: String,
    /// The app's executable: the bot's shell runs it with `--cli` to reach Lynx's tools.
    cli: Option<PathBuf>,
    /// The environment that lets that CLI reach the app as this turn's session.
    env: Vec<(String, String)>,
}

impl ClaudeCli {
    pub fn new(bots_dir: PathBuf, home: PathBuf) -> Self {
        Self {
            bots_dir,
            home,
            bridge: OnceLock::new(),
            running: Mutex::new(HashMap::new()),
        }
    }

    /// Stops the answer a bot is writing in one thread (or channel). Returns whether there was one.
    /// What it had written is kept.
    pub fn stop(&self, bot: &str, label: &str) -> bool {
        let running = self.running.lock().unwrap_or_else(PoisonError::into_inner);
        running.get(&session_of(bot, label)).is_some_and(|flag| {
            flag.store(true, Ordering::SeqCst);
            true
        })
    }

    /// Connects the bots to the app: their MCP server and the repositories they may read.
    pub fn attach(&self, bridge: BotBridge) {
        let _ = self.bridge.set(bridge);
    }

    /// What a bot's run may reach: the servers and skills in its folder, and the folders of the
    /// repositories it was granted. Lynx's own server and those folders need the bridge.
    fn reach_of(
        &self,
        session: &str,
        dir: &Path,
        grants: &[super::bot_access::Grant],
        commands: &[String],
    ) -> Result<Reach, ClaudeCliError> {
        let mut reach = Reach {
            servers: bot_mcp::servers(dir)?,
            skills: bot_skills::any(dir),
            commands: commands.to_vec(),
            ..Reach::default()
        };
        if let Some(bridge) = self.bridge.get() {
            reach.read_dirs = grants
                .iter()
                .flat_map(|grant| bridge.repos().folders_of(&grant.id))
                .collect();
            reach.cli = Some(bridge.executable().to_path_buf());
            reach.env = bridge.session_env(session);
            // The lynx skill is in the bot's folder, so the bot runs its skills.
            reach.skills = true;
        }
        Ok(reach)
    }

    /// One question, one answer. Never fails: a problem comes back as the text. Only the tests ask this way.
    #[cfg(test)]
    pub fn ask(&self, bot: &str, message: &str) -> ClaudeAnswer {
        self.ask_streaming(bot, "test", message, &mut |_| {})
    }

    /// One question to the CLI in `dir`, with `instructions`, for the channel router. Never fails.
    pub fn ask_in(&self, dir: &Path, instructions: &str, message: &str) -> ClaudeAnswer {
        self.run_in(
            dir,
            instructions,
            None,
            &Reach::default(),
            message,
            &mut |_| {},
        )
        .unwrap_or_else(|error| ClaudeAnswer {
            ok: false,
            text: error.to_string(),
            stopped: false,
        })
    }

    /// Like `ask`, but `on_text` gets each piece of the reply as the CLI writes it.
    pub fn ask_streaming(
        &self,
        bot: &str,
        label: &str,
        message: &str,
        on_text: &mut dyn FnMut(&str),
    ) -> ClaudeAnswer {
        self.run(bot, label, message, on_text)
            .unwrap_or_else(|error| ClaudeAnswer {
                ok: false,
                text: error.to_string(),
                stopped: false,
            })
    }

    fn run(
        &self,
        bot: &str,
        label: &str,
        message: &str,
        on_text: &mut dyn FnMut(&str),
    ) -> Result<ClaudeAnswer, ClaudeCliError> {
        let folder = BotFolder::open(&self.bots_dir, bot)?;
        let settings = folder.settings()?;
        if settings.provider != "claude" {
            return Ok(ClaudeAnswer {
                ok: false,
                text: format!(
                    "This bot answers with {}, which Lynx cannot reach yet.",
                    settings.provider
                ),
                stopped: false,
            });
        }
        let instructions = folder.instructions()?;
        // The session lives for the whole run: the bot's tool calls reach the app only while it does.
        let session = session_of(bot, label);
        let _session = self
            .bridge
            .get()
            .map(|bridge| bridge.open_session(&session));
        let cancel = Arc::new(AtomicBool::new(false));
        self.running
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(session.clone(), Arc::clone(&cancel));
        let _registered = Registered {
            running: &self.running,
            session: &session,
        };
        let reach = Reach {
            cancel: Arc::clone(&cancel),
            label: label.to_owned(),
            ..self.reach_of(&session, folder.dir(), &settings.repos, &settings.commands)?
        };
        self.run_in(
            folder.dir(),
            &system_prompt_of(folder.dir(), &instructions),
            settings.model.as_deref(),
            &reach,
            message,
            on_text,
        )
    }

    /// Runs the CLI in `dir` with `system_prompt` in place of its own prompt: a bot's folder, or a
    /// channel's folder for the router.
    fn run_in(
        &self,
        dir: &Path,
        system_prompt: &str,
        model: Option<&str>,
        reach: &Reach,
        message: &str,
        on_text: &mut dyn FnMut(&str),
    ) -> Result<ClaudeAnswer, ClaudeCliError> {
        let run_dir = dir.join(RUN_DIR).join(folder_name(&reach.label));
        fs::create_dir_all(&run_dir)?;
        let cli_settings = run_dir.join("claude-settings.json");
        let mcp = run_dir.join("mcp.json");
        let prompt_file = run_dir.join("system-prompt.txt");
        let excludes = claude_md_excludes(dir, &self.home);
        fs::write(&cli_settings, cli_settings_json(excludes).to_string())?;
        // The MCP config holds the hub's token and the servers' secrets, so only its owner can read it.
        private_file::write(
            &mcp,
            json!({ "mcpServers": reach.servers })
                .to_string()
                .as_bytes(),
        )?;
        // One step, so a run that starts while another writes the same file never reads half of it.
        private_file::write(&prompt_file, system_prompt.as_bytes())?;
        if let Some(cli) = &reach.cli {
            // The lynx skill says how to call the CLI with the tools this bot has.
            skill_file::write(
                &dir.join(".claude").join("skills"),
                &skill_file::render(
                    cli,
                    skill_file::BOT_PURPOSE,
                    &list_for(Some(BOT_SESSION_PREFIX)),
                    &bot_instructions(),
                ),
            )?;
        }

        // A server the bot has is callable by its name. Lynx's own tools come through its shell, not MCP.
        let mcp_tools: Vec<String> = reach
            .servers
            .keys()
            .map(|name| format!("mcp__{name}"))
            .collect();
        let launch = Launch {
            settings: &cli_settings,
            mcp: &mcp,
            read_dirs: &reach.read_dirs,
            mcp_tools: &mcp_tools,
            skills: reach.skills,
            commands: &reach.commands,
            cli: reach.cli.as_deref(),
        };
        let mut command = Command::new("claude");
        command
            .current_dir(dir)
            .envs(reach.env.iter().map(|(key, value)| (key, value)))
            .args(bot_arguments(message, &prompt_file, model, &launch));
        clean_command(&mut command);
        let mut answer = None;
        let mut written = String::new();
        let mut on_line = |line: &str| match stream_part(line) {
            Some(StreamPart::Text(text)) => {
                written.push_str(&text);
                on_text(&text);
            }
            Some(StreamPart::Done(done)) => answer = Some(done),
            None => {}
        };
        let finished = run_streaming(&mut command, ANSWER_TIMEOUT, &reach.cancel, &mut on_line)
            .map_err(|error| match error {
                BoundedError::TimedOut => ClaudeCliError::TimedOut,
                BoundedError::Spawn(error) if error.kind() == ErrorKind::NotFound => {
                    ClaudeCliError::NotInstalled
                }
                BoundedError::Spawn(error) => ClaudeCliError::Io(error),
            })?;
        if finished.is_none() {
            return Ok(ClaudeAnswer {
                ok: true,
                text: written,
                stopped: true,
            });
        }
        Ok(answer.unwrap_or_else(|| ClaudeAnswer {
            ok: false,
            text: "Claude did not answer.".to_owned(),
            stopped: false,
        }))
    }
}

/// The name of a turn's folder: letters, digits and dashes, so a label can never leave `.run/`.
fn folder_name(label: &str) -> String {
    let name: String = label
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if name.is_empty() {
        "turn".to_owned()
    } else {
        name
    }
}

/// Takes a turn off the list of those answering when its run ends, however it ends.
struct Registered<'a> {
    running: &'a Mutex<HashMap<String, Arc<AtomicBool>>>,
    session: &'a str,
}

impl Drop for Registered<'_> {
    fn drop(&mut self) {
        self.running
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(self.session);
    }
}

/// What the bot's skill says about its tools: how they are called and how a coordinator works with its
/// workers. A bot has no browser, so the browser's instructions stay out.
fn bot_instructions() -> String {
    format!(
        "{} {}",
        bot_tools::INSTRUCTIONS,
        orchestration_tools::INSTRUCTIONS
    )
}

/// The bot's system prompt: the base, its instructions, and what it remembers.
fn system_prompt_of(bot_dir: &Path, instructions: &str) -> String {
    let prompt = base_instructions::with_base(instructions);
    [
        bot_memory::prompt_section(bot_dir),
        bot_topics::index_section(bot_dir),
    ]
    .into_iter()
    .flatten()
    .fold(prompt, |prompt, section| format!("{prompt}\n\n{section}"))
}

/// The files one run is given: its settings, its MCP config, the folders it may read, the MCP tools
/// it may call, and whether it loads the bot's skills.
pub struct Launch<'a> {
    pub settings: &'a Path,
    pub mcp: &'a Path,
    pub read_dirs: &'a [PathBuf],
    pub mcp_tools: &'a [String],
    pub skills: bool,
    /// The command patterns the run may execute.
    pub commands: &'a [String],
    /// The app's executable, which the run may call with `--cli` and nothing else from its shell.
    pub cli: Option<&'a Path>,
}

/// The arguments that make the CLI a bot: its system prompt read from `system_prompt` in place of its
/// own prompt, and only the tools the launch allows. A bot with no granted folder has no built-in
/// tools, so it cannot read, write or run anything; one with a granted folder can read and search
/// it, and nothing else. A bot with skills also gets the tool that runs them, and the CLI loads only
/// the skills in the bot's folder. Sign-in is untouched: `--bare` would also turn off the subscription login.
pub fn bot_arguments(
    message: &str,
    system_prompt: &Path,
    model: Option<&str>,
    launch: &Launch,
) -> Vec<String> {
    let mut tools: Vec<&str> = Vec::new();
    if !launch.read_dirs.is_empty() {
        tools.extend(READ_TOOLS);
    }
    if launch.skills {
        tools.push(SKILL_TOOL);
    }
    if (launch.cli.is_some() || !launch.commands.is_empty()) && !tools.contains(&BASH_TOOL) {
        tools.push(BASH_TOOL);
    }
    let mut arguments = vec![
        "-p".to_owned(),
        message.to_owned(),
        "--system-prompt-file".to_owned(),
        system_prompt.display().to_string(),
        "--tools".to_owned(),
        tools.join(","),
        "--setting-sources".to_owned(),
        "project".to_owned(),
        "--settings".to_owned(),
        launch.settings.display().to_string(),
        "--strict-mcp-config".to_owned(),
        "--mcp-config".to_owned(),
        launch.mcp.display().to_string(),
        "--no-session-persistence".to_owned(),
        "--output-format".to_owned(),
        "stream-json".to_owned(),
        "--verbose".to_owned(),
        "--include-partial-messages".to_owned(),
    ];
    // Without skills every skill stays off, as before; with them, only the bot's own are loaded.
    if !launch.skills {
        arguments.push("--disable-slash-commands".to_owned());
    }
    if let Some(model) = model {
        arguments.extend(["--model".to_owned(), model.to_owned()]);
    }
    // The list flags take several values, so they come last, where no flag can follow them.
    if !launch.read_dirs.is_empty() {
        arguments.push("--disallowedTools".to_owned());
        arguments.extend(SECRET_DENIALS.iter().map(|rule| (*rule).to_owned()));
    }
    let mut allowed: Vec<String> = Vec::new();
    if let Some(cli) = launch.cli {
        allowed.push(format!("Bash({} --cli *)", cli.display()));
    }
    allowed.extend(launch.mcp_tools.to_vec());
    allowed.extend(command_permissions::rules(launch.commands));
    allowed.extend(
        launch
            .read_dirs
            .iter()
            .map(|dir| format!("Read({}/**)", dir.display())),
    );
    if !allowed.is_empty() {
        arguments.push("--allowedTools".to_owned());
        arguments.extend(allowed);
    }
    if !launch.read_dirs.is_empty() {
        arguments.push("--add-dir".to_owned());
        arguments.extend(launch.read_dirs.iter().map(|dir| dir.display().to_string()));
    }
    arguments
}

/// The settings a run is given: the memory files it must not read, and none of the CLI's bundled
/// skills. A bot gets only the skills in its folder. `plugin-authoring` stays listed under
/// `disableBundledSkills` on CLI 2.1.310, so it is also switched off by name.
fn cli_settings_json(excludes: Vec<String>) -> Value {
    json!({
        "claudeMdExcludes": excludes,
        "disableBundledSkills": true,
        "skillOverrides": { "plugin-authoring": "off" },
    })
}

/// The CLI's memory files that would otherwise reach the bot: the user's, and the `CLAUDE.md` of the
/// bot's folder and of every folder above it. Excluded by path, since `--setting-sources` does not
/// cover them.
pub fn claude_md_excludes(workdir: &Path, home: &Path) -> Vec<String> {
    let mut paths = vec![home.join(".claude").join("CLAUDE.md")];
    for dir in workdir.ancestors() {
        paths.push(dir.join("CLAUDE.md"));
        paths.push(dir.join(".claude").join("CLAUDE.md"));
    }
    paths
        .into_iter()
        .map(|path| path.display().to_string())
        .collect()
}

#[derive(Deserialize)]
struct Turn {
    is_error: bool,
    #[serde(default)]
    subtype: String,
    result: Option<String>,
}

/// One piece of the CLI's stream: a bit of the reply as it is written, or the final answer.
#[derive(Debug, PartialEq, Eq)]
pub enum StreamPart {
    Text(String),
    Done(ClaudeAnswer),
}

/// Reads one line of `--output-format stream-json`. Lines that carry neither a piece of the reply nor
/// the answer (setup, tool and rate-limit events) give nothing.
pub fn stream_part(line: &str) -> Option<StreamPart> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    match value.get("type")?.as_str()? {
        "stream_event" => {
            let delta = value.get("event")?.get("delta")?;
            if delta.get("type")?.as_str()? != "text_delta" {
                return None;
            }
            Some(StreamPart::Text(delta.get("text")?.as_str()?.to_owned()))
        }
        "result" => {
            let turn: Turn = serde_json::from_value(value).ok()?;
            Some(StreamPart::Done(answer_of(turn)))
        }
        _ => None,
    }
}

fn answer_of(turn: Turn) -> ClaudeAnswer {
    let (ok, text) = match turn.result {
        Some(text) if !turn.is_error && turn.subtype == "success" => (true, text),
        Some(text) => (false, text),
        None => (false, format!("Claude ended with {}.", turn.subtype)),
    };
    ClaudeAnswer {
        ok,
        text,
        stopped: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value_after<'a>(arguments: &'a [String], flag: &str) -> Option<&'a str> {
        arguments
            .iter()
            .position(|argument| argument == flag)
            .and_then(|index| arguments.get(index + 1))
            .map(String::as_str)
    }

    fn plain_launch() -> Launch<'static> {
        Launch {
            settings: Path::new("/w/s.json"),
            mcp: Path::new("/w/m.json"),
            read_dirs: &[],
            mcp_tools: &[],
            skills: false,
            commands: &[],
            cli: None,
        }
    }

    #[test]
    fn a_bot_with_a_granted_folder_reads_it_and_nothing_else() {
        let read = [PathBuf::from("/repos/lynx")];
        let tools = ["mcp__lynx-bot__bot_request_access".to_owned()];
        let launch = Launch {
            settings: Path::new("/w/s.json"),
            mcp: Path::new("/w/m.json"),
            read_dirs: &read,
            mcp_tools: &tools,
            skills: false,
            commands: &[],
            cli: None,
        };

        let arguments = bot_arguments("oi", Path::new("/w/system.txt"), None, &launch);

        assert_eq!(value_after(&arguments, "--tools"), Some("Read,Glob,Grep"));
        assert!(arguments.contains(&"Read(/repos/lynx/**)".to_owned()));
        assert!(arguments.contains(&"mcp__lynx-bot__bot_request_access".to_owned()));
        assert!(
            arguments.contains(&"Read(**/.env)".to_owned()),
            "secrets are denied"
        );
        assert_eq!(value_after(&arguments, "--add-dir"), Some("/repos/lynx"));
        assert!(!arguments.contains(&"Bash".to_owned()));
    }

    #[test]
    fn a_bot_without_grants_has_no_built_in_tools_and_no_read_rules() {
        let arguments = bot_arguments("oi", Path::new("/w/system.txt"), None, &plain_launch());

        assert_eq!(value_after(&arguments, "--tools"), Some(""));
        assert!(!arguments.contains(&"--add-dir".to_owned()));
        assert!(!arguments.contains(&"--disallowedTools".to_owned()));
    }

    #[test]
    fn the_bot_runs_with_its_system_prompt_file_and_no_tools() {
        let arguments = bot_arguments("oi", Path::new("/w/system.txt"), None, &plain_launch());

        assert_eq!(
            value_after(&arguments, "--system-prompt-file"),
            Some("/w/system.txt")
        );
        assert!(
            !arguments.contains(&"--system-prompt".to_owned()),
            "the prompt goes in a file, off the command line"
        );
        assert_eq!(value_after(&arguments, "--tools"), Some(""));
        assert_eq!(
            value_after(&arguments, "--setting-sources"),
            Some("project")
        );
        assert!(arguments.contains(&"--strict-mcp-config".to_owned()));
        assert!(arguments.contains(&"--disable-slash-commands".to_owned()));
        assert!(
            !arguments.contains(&"--bare".to_owned()),
            "--bare turns off the subscription login"
        );
        assert!(
            !arguments.contains(&"--model".to_owned()),
            "no model unless the bot names one"
        );
    }

    #[test]
    fn what_the_bot_remembers_follows_its_instructions_in_the_prompt() {
        let dir = tempfile::tempdir().expect("temp dir");
        assert_eq!(
            system_prompt_of(dir.path(), "Ajude."),
            base_instructions::with_base("Ajude."),
            "no memory, nothing added"
        );

        bot_memory::add(dir.path(), "gosta de café", "2026-10-26").expect("added");
        let prompt = system_prompt_of(dir.path(), "Ajude.");
        let (instructions, memory) = (prompt.find("Ajude."), prompt.find("gosta de café"));
        assert!(instructions < memory && memory.is_some(), "{prompt}");
    }

    #[test]
    fn a_bot_with_skills_gets_the_skill_tool_and_the_cli_loads_its_own_skills() {
        let launch = Launch {
            skills: true,
            ..plain_launch()
        };

        let arguments = bot_arguments("oi", Path::new("/w/system.txt"), None, &launch);

        assert_eq!(value_after(&arguments, "--tools"), Some("Skill"));
        assert!(
            !arguments.contains(&"--disable-slash-commands".to_owned()),
            "the flag would switch the bot's skills off too"
        );
        assert!(!arguments.contains(&"Bash".to_owned()));
    }

    #[test]
    fn a_bot_with_skills_and_a_granted_folder_gets_both_sets_of_tools() {
        let read = [PathBuf::from("/repos/lynx")];
        let launch = Launch {
            read_dirs: &read,
            skills: true,
            ..plain_launch()
        };

        let arguments = bot_arguments("oi", Path::new("/w/system.txt"), None, &launch);

        assert_eq!(
            value_after(&arguments, "--tools"),
            Some("Read,Glob,Grep,Skill")
        );
    }

    #[test]
    fn a_bot_with_command_patterns_gets_bash_and_only_those_patterns() {
        let commands = ["minha-cli *".to_owned(), "git status".to_owned()];
        let launch = Launch {
            commands: &commands,
            ..plain_launch()
        };

        let arguments = bot_arguments("oi", Path::new("/w/system.txt"), None, &launch);

        assert_eq!(value_after(&arguments, "--tools"), Some("Bash"));
        let allowed = arguments
            .iter()
            .skip_while(|argument| *argument != "--allowedTools")
            .skip(1)
            .collect::<Vec<_>>();
        assert_eq!(allowed, ["Bash(minha-cli *)", "Bash(git status)"]);
        assert!(
            !arguments.contains(&"Bash".to_owned())
                || value_after(&arguments, "--tools") == Some("Bash")
        );
    }

    #[test]
    fn a_bot_without_command_patterns_has_no_bash() {
        let arguments = bot_arguments("oi", Path::new("/w/system.txt"), None, &plain_launch());

        assert_eq!(value_after(&arguments, "--tools"), Some(""));
        assert!(
            !arguments
                .iter()
                .any(|argument| argument.starts_with("Bash"))
        );
    }

    #[test]
    fn a_server_the_bot_has_is_callable_by_its_name() {
        let tools = ["mcp__github".to_owned()];
        let launch = Launch {
            mcp_tools: &tools,
            ..plain_launch()
        };

        let arguments = bot_arguments("oi", Path::new("/w/system.txt"), None, &launch);

        assert_eq!(
            value_after(&arguments, "--allowedTools"),
            Some("mcp__github")
        );
    }

    #[test]
    fn the_run_switches_off_the_cli_bundled_skills_but_keeps_the_memory_excludes() {
        let settings = cli_settings_json(vec!["/home/u/.claude/CLAUDE.md".to_owned()]);

        assert_eq!(settings["disableBundledSkills"], true);
        assert_eq!(settings["skillOverrides"]["plugin-authoring"], "off");
        assert_eq!(
            settings["claudeMdExcludes"],
            json!(["/home/u/.claude/CLAUDE.md"])
        );
    }

    #[test]
    fn a_model_named_in_the_bot_settings_is_passed_on() {
        let arguments = bot_arguments(
            "oi",
            Path::new("/w/system.txt"),
            Some("claude-haiku-5-5"),
            &plain_launch(),
        );

        assert_eq!(value_after(&arguments, "--model"), Some("claude-haiku-5-5"));
    }

    #[test]
    fn the_user_and_every_folder_above_the_bot_are_excluded_from_memory() {
        // Windows joins with a backslash; the list is compared with slashes.
        let excludes: Vec<String> =
            claude_md_excludes(Path::new("/home/u/.lynx/bots/ada"), Path::new("/home/u"))
                .iter()
                .map(|path| path.replace('\\', "/"))
                .collect();

        assert!(excludes.contains(&"/home/u/.claude/CLAUDE.md".to_owned()));
        assert!(excludes.contains(&"/home/u/.lynx/bots/CLAUDE.md".to_owned()));
        assert!(excludes.contains(&"/home/u/CLAUDE.md".to_owned()));
        assert!(excludes.contains(&"/home/u/.lynx/bots/ada/.claude/CLAUDE.md".to_owned()));
    }

    #[test]
    fn a_text_piece_is_part_of_the_reply_as_it_is_written() {
        let piece = r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"Oi"}}}"#;
        assert_eq!(stream_part(piece), Some(StreamPart::Text("Oi".to_owned())));

        let thinking = r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"thinking_delta","thinking":"hum"}}}"#;
        assert_eq!(stream_part(thinking), None);
        assert_eq!(stream_part(r#"{"type":"rate_limit_event"}"#), None);
        assert_eq!(stream_part("not json"), None);
    }

    #[test]
    fn the_result_line_is_the_answer_or_the_reason_for_none() {
        let ok =
            stream_part(r#"{"type":"result","subtype":"success","is_error":false,"result":"Oi!"}"#);
        assert_eq!(
            ok,
            Some(StreamPart::Done(ClaudeAnswer {
                ok: true,
                text: "Oi!".to_owned(),
                stopped: false,
            }))
        );

        let refused = stream_part(
            r#"{"type":"result","subtype":"error","is_error":true,"result":"Not logged in"}"#,
        );
        assert_eq!(
            refused,
            Some(StreamPart::Done(ClaudeAnswer {
                ok: false,
                text: "Not logged in".to_owned(),
                stopped: false,
            }))
        );
    }

    #[test]
    fn the_answer_is_asked_as_a_stream_with_partial_pieces() {
        let arguments = bot_arguments("oi", Path::new("/w/system.txt"), None, &plain_launch());

        assert_eq!(
            value_after(&arguments, "--output-format"),
            Some("stream-json")
        );
        assert!(arguments.contains(&"--verbose".to_owned()));
        assert!(arguments.contains(&"--include-partial-messages".to_owned()));
    }

    /// An MCP server with one tool, `ecoar`, that answers with its text behind `ECO-`.
    const ECHO_SERVER: &str = r#"
import { createInterface } from "node:readline";
const tools = [{ name: "ecoar", description: "Devolve o texto com o prefixo ECO-", inputSchema: { type: "object", properties: { texto: { type: "string" } }, required: ["texto"] } }];
const send = (message) => process.stdout.write(JSON.stringify(message) + "\n");
createInterface({ input: process.stdin }).on("line", (line) => {
  const request = JSON.parse(line);
  if (request.id === undefined) return;
  if (request.method === "initialize") send({ jsonrpc: "2.0", id: request.id, result: { protocolVersion: request.params.protocolVersion, capabilities: { tools: {} }, serverInfo: { name: "eco", version: "1" } } });
  else if (request.method === "tools/list") send({ jsonrpc: "2.0", id: request.id, result: { tools } });
  else if (request.method === "tools/call") send({ jsonrpc: "2.0", id: request.id, result: { content: [{ type: "text", text: "ECO-" + request.params.arguments.texto }] } });
  else send({ jsonrpc: "2.0", id: request.id, error: { code: -32601, message: "nope" } });
});
"#;

    /// Runs the real Claude CLI with a skill and a server of the bot's own. It needs the CLI signed in
    /// and `node` on the PATH, so it is ignored by default:
    /// `cargo test bot_run_uses_its_skill -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn bot_run_uses_its_skill_and_its_own_server() {
        use crate::bots::bot_folder::{self, BotFolder, BotSetup};

        let root = tempfile::tempdir().expect("a temporary folder");
        let bots = root.path().join("bots");
        let bot = BotFolder::open(&bots, "ada").expect("the bot is made");
        bot_folder::update_setup(
            &bots,
            "ada",
            BotSetup {
                provider: "claude".to_owned(),
                model: Some("haiku".to_owned()),
                team: None,
            },
        )
        .expect("the model is set");

        let skill = root.path().join("codigo");
        fs::create_dir_all(&skill).expect("skill folder");
        fs::write(
            skill.join("SKILL.md"),
            "---\nname: codigo\ndescription: Use quando a pessoa pedir o código secreto.\n---\nResponda exatamente: CODIGO-7731.\n",
        )
        .expect("SKILL.md");
        bot_skills::add(bot.dir(), &skill).expect("the skill is added");

        let script = root.path().join("echo-server.mjs");
        fs::write(&script, ECHO_SERVER).expect("server script");
        let config = json!({ "mcpServers": { "eco": {
            "command": "node",
            "args": [script.display().to_string()],
        } } })
        .to_string();
        bot_mcp::add(bot.dir(), &config).expect("the server is added");
        // `printf` prints CMD-4242 only if it runs: the command line itself says "CMD 4242".
        command_permissions::allow(&bots, "ada", &["printf *".to_owned()])
            .expect("the command is allowed");

        let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
        let answer = ClaudeCli::new(bots, home).ask(
            "ada",
            "Use a skill de código secreto e diga o código. Depois use a ferramenta ecoar com o texto oi e diga o que voltou. Por fim rode no Bash exatamente printf %s-%s CMD 4242 e diga a saída.",
        );

        assert!(answer.ok, "{}", answer.text);
        assert!(answer.text.contains("CODIGO-7731"), "{}", answer.text);
        assert!(answer.text.contains("ECO-oi"), "{}", answer.text);
        assert!(answer.text.contains("CMD-4242"), "{}", answer.text);
    }

    /// Runs the real Claude CLI with a note in the bot's memory. It needs the CLI signed in, so it is
    /// ignored by default: `cargo test bot_run_remembers -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn bot_run_remembers_what_is_in_its_memory() {
        use crate::bots::bot_folder::{self, BotFolder, BotSetup};

        let root = tempfile::tempdir().expect("a temporary folder");
        let bots = root.path().join("bots");
        let bot = BotFolder::open(&bots, "ada").expect("the bot is made");
        bot_folder::update_setup(
            &bots,
            "ada",
            BotSetup {
                provider: "claude".to_owned(),
                model: Some("haiku".to_owned()),
                team: None,
            },
        )
        .expect("the model is set");
        bot_memory::add(
            bot.dir(),
            "A cor favorita da pessoa é turquesa-7421.",
            "2026-10-26",
        )
        .expect("a note");

        let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
        let answer = ClaudeCli::new(bots, home)
            .ask("ada", "Qual é a minha cor favorita? Responda só com a cor.");

        assert!(answer.ok, "{}", answer.text);
        assert!(answer.text.contains("turquesa-7421"), "{}", answer.text);
    }

    /// Asks the real Claude CLI what is worth remembering in a short chat and files it. It needs the CLI
    /// signed in, so it is ignored by default: `cargo test bot_capture -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn bot_capture_files_what_a_chat_says_with_the_real_cli() {
        use crate::bots::bot_folder::BotFolder;
        use crate::bots::{bot_memory_capture, bot_topics};

        let root = tempfile::tempdir().expect("a temporary folder");
        let bots = root.path().join("bots");
        let bot = BotFolder::open(&bots, "ada").expect("the bot is made");
        let turns = "Pessoa: Oi! Eu me chamo Marina e sou vegetariana.\n\nAda: Prazer, Marina! Anotado.\n\n\
                     Pessoa: Vou pra Lisboa dia 12 de dezembro, o hotel fica no Chiado. Me ajuda com um roteiro?\n\n\
                     Ada: Claro! Quer focar em museus ou em comida?\n\nPessoa: Comida. E como está o tempo hoje?";
        let question = bot_memory_capture::prompt("", &[], turns);

        let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
        let answer = ClaudeCli::new(bots, home).ask_in(
            bot.dir(),
            bot_memory_capture::INSTRUCTIONS,
            &question,
        );
        println!("RAW: {}", answer.text);
        let notes = bot_memory_capture::parse(&answer.text);
        println!("PARSED: {notes:#?}");
        let filed = bot_memory_capture::apply(bot.dir(), &notes, "2026-10-27");
        println!("MEMORY:\n{}", bot_memory::read(bot.dir()));
        println!("TOPICS: {:?}", bot_topics::list(bot.dir()));

        assert!(answer.ok, "{}", answer.text);
        assert!(filed >= 1, "nothing was filed: {}", answer.text);
    }
}

#[cfg(test)]
mod cli_tests {
    use super::*;

    fn value_after<'a>(arguments: &'a [String], flag: &str) -> Option<&'a str> {
        arguments
            .iter()
            .position(|argument| argument == flag)
            .and_then(|index| arguments.get(index + 1))
            .map(String::as_str)
    }

    #[test]
    fn a_bot_gets_bash_only_for_the_lynx_cli_and_its_skill() {
        let launch = Launch {
            settings: Path::new("/w/s.json"),
            mcp: Path::new("/w/m.json"),
            read_dirs: &[],
            mcp_tools: &[],
            skills: true,
            commands: &[],
            cli: Some(Path::new("/opt/lynx/lynx-ide")),
        };

        let arguments = bot_arguments("oi", Path::new("/w/system.txt"), None, &launch);

        let tools = value_after(&arguments, "--tools").expect("a tools list");
        assert_eq!(tools, "Skill,Bash");
        let allowed = arguments
            .iter()
            .position(|argument| argument == "--allowedTools")
            .map(|index| &arguments[index + 1..])
            .expect("an allow list");
        assert_eq!(allowed[0], "Bash(/opt/lynx/lynx-ide --cli *)");
        assert_eq!(allowed.len(), 1, "no other shell command is pre-approved");
    }

    #[test]
    fn bash_is_listed_once_when_the_cli_and_command_patterns_are_both_on() {
        let commands = ["minha-cli *".to_owned()];
        let launch = Launch {
            settings: Path::new("/w/s.json"),
            mcp: Path::new("/w/m.json"),
            read_dirs: &[],
            mcp_tools: &[],
            skills: true,
            commands: &commands,
            cli: Some(Path::new("/opt/lynx/lynx-ide")),
        };

        let arguments = bot_arguments("oi", Path::new("/w/system.txt"), None, &launch);

        assert_eq!(value_after(&arguments, "--tools"), Some("Skill,Bash"));
    }
}
