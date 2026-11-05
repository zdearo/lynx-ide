//! A bot's own folder, under `LYNX_HOME/bots/<id>/`. Its instructions and settings are plain files
//! the user can edit. Their names belong to Lynx, not to one provider, so a bot's files stay the
//! same when its provider changes.

use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use specta::Type;

use super::bot_access::Grant;
use crate::private_file;

/// The bot that exists until the bots screen can create more.
#[cfg(test)]
pub const DEFAULT_BOT: &str = "ada";
/// The colours a bot can wear. The names match the ones the settings screen offers.
pub const COLORS: [&str; 8] = [
    "rose", "sky", "indigo", "violet", "amber", "green", "teal", "graphite",
];
/// The providers a bot can name. Only Claude answers so far; the others are reserved.
pub const PROVIDERS: [&str; 3] = ["claude", "chatgpt", "grok"];
const INSTRUCTIONS_FILE: &str = "INSTRUCTIONS.md";
const SETTINGS_FILE: &str = "bot.json";
const MAX_NAME: usize = 40;
const MAX_INSTRUCTIONS: usize = 8000;
const MAX_MODEL: usize = 80;
const MAX_TEAM: usize = 24;
/// Written once, when the folder is first made. After that the file belongs to the user.
const DEFAULT_INSTRUCTIONS: &str = "Você é a Ada, uma assistente pessoal de rotina. Você não é um agente de programação: não escreve nem altera código, e responde em português, de forma breve.";

/// What the bot's `bot.json` says. A file that lacks a field gets the default for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BotSettings {
    pub name: String,
    pub color: String,
    pub provider: String,
    pub model: Option<String>,
    /// The team the bot belongs to, a label shown beside it. Absent: no team.
    pub team: Option<String>,
    /// The registered repositories the person let this bot read.
    pub repos: Vec<Grant>,
    /// The command patterns the person let this bot run, like `minha-cli *`.
    pub commands: Vec<String>,
    /// The registered repositories where the person let this bot start agents.
    pub workers: Vec<Grant>,
}

impl Default for BotSettings {
    fn default() -> Self {
        Self {
            name: "Ada".to_owned(),
            color: "rose".to_owned(),
            provider: "claude".to_owned(),
            model: None,
            team: None,
            repos: Vec::new(),
            commands: Vec::new(),
            workers: Vec::new(),
        }
    }
}

/// A bot as the bots screen lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BotSummary {
    pub id: String,
    pub name: String,
    pub color: String,
    pub provider: String,
    pub model: Option<String>,
    pub team: Option<String>,
}

/// What the form asks for when a bot is created.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct NewBot {
    pub name: String,
    pub color: String,
    pub provider: String,
    pub model: Option<String>,
    pub instructions: String,
    pub team: Option<String>,
}

/// A bot as its details tab shows it: the summary, the instructions the user wrote, and its folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BotDetail {
    pub id: String,
    pub name: String,
    pub color: String,
    pub provider: String,
    pub model: Option<String>,
    pub instructions: String,
    pub folder: String,
    pub team: Option<String>,
}

/// What the details tab changes in a bot's setup: the engine, the model it names, and its team.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BotSetup {
    pub provider: String,
    pub model: Option<String>,
    pub team: Option<String>,
}

/// Where every bot's folder lives, so the commands can find them.
pub struct BotsRoot(pub PathBuf);

#[derive(Debug, thiserror::Error)]
pub enum BotError {
    #[error("the bot name needs 1 to 40 characters, with letters or digits")]
    InvalidName,
    #[error("that colour is not one Lynx offers")]
    InvalidColor,
    #[error("that provider is not one Lynx knows")]
    InvalidProvider,
    #[error("the instructions need 1 to 8000 characters")]
    InvalidInstructions,
    #[error("the bot is answering: change it once its reply is written")]
    Busy,
    #[error("there is no channel with that id")]
    UnknownChannel,
    #[error("a channel needs a name of 1 to 40 characters, with letters or digits")]
    InvalidChannel,
    #[error("a channel with that name already exists")]
    ChannelExists,
    #[error("the channel is answering: wait until its reply is written")]
    ChannelBusy,
    #[error("there is no access request waiting for an answer with that id")]
    UnknownRequest,
    #[error("that project is no longer registered in Lynx")]
    UnknownRepo,
    #[error("the model name is longer than 80 characters")]
    InvalidModel,
    #[error("the team name is longer than 24 characters")]
    InvalidTeam,
    #[error("that folder is not a skill: it needs a SKILL.md with a name and a description")]
    InvalidSkill,
    #[error("the skill has a link, which Lynx does not copy")]
    SkillLink,
    #[error("a skill can have up to 200 files and 2 MB")]
    SkillTooLarge,
    #[error("the bot already has a skill with that name")]
    SkillExists,
    #[error("the bot has no skill with that name")]
    UnknownSkill,
    #[error("the name lynx is kept for the skill Lynx writes for the bot's tools")]
    ReservedSkill,
    #[error("the MCP servers are not valid: {0}")]
    InvalidMcp(String),
    #[error("the bot has no MCP server with that name")]
    UnknownMcp,
    #[error("the key is empty, has spaces, or is longer than 512 characters")]
    InvalidKey,
    #[error("that command pattern is not allowed: {0}")]
    InvalidCommand(String),
    #[error("the memory note is not valid: {0}")]
    InvalidMemory(String),
    #[error("there is no thread with that id")]
    UnknownThread,
    #[error("a thread title has up to 60 characters")]
    InvalidThreadTitle,
    #[error("could not read or write the bot's folder: {0}")]
    Io(#[from] io::Error),
    #[error("there is no bot with that id")]
    UnknownBot,
    #[error("a message needs 1 to 4000 characters")]
    InvalidMessage,
    #[error("the conversation file cannot be read, so it was left as it is: {0}")]
    Conversation(#[from] serde_json::Error),
}

pub struct BotFolder {
    dir: PathBuf,
}

impl BotFolder {
    /// Opens a bot's folder, making it with its default files when it does not exist yet.
    pub fn open(bots_dir: &Path, id: &str) -> io::Result<Self> {
        if !valid_id(id) {
            return Err(io::Error::new(ErrorKind::InvalidInput, "invalid bot id"));
        }
        let dir = bots_dir.join(id);
        fs::create_dir_all(&dir)?;
        write_if_missing(&dir.join(INSTRUCTIONS_FILE), DEFAULT_INSTRUCTIONS)?;
        let settings =
            serde_json::to_string_pretty(&BotSettings::default()).map_err(io::Error::other)?;
        write_if_missing(&dir.join(SETTINGS_FILE), &settings)?;
        Ok(Self { dir })
    }

    /// Opens a bot that exists. Unlike `open`, it never makes a folder, so a wrong id cannot make a bot.
    pub fn existing(bots_dir: &Path, id: &str) -> Result<Self, BotError> {
        if !valid_id(id) || !bots_dir.join(id).join(SETTINGS_FILE).is_file() {
            return Err(BotError::UnknownBot);
        }
        Ok(Self {
            dir: bots_dir.join(id),
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn instructions(&self) -> io::Result<String> {
        fs::read_to_string(self.dir.join(INSTRUCTIONS_FILE))
    }

    pub fn settings(&self) -> io::Result<BotSettings> {
        read_settings(&self.dir.join(SETTINGS_FILE))
    }
}

/// Every bot in `bots_dir`. A folder whose `bot.json` cannot be read is left out, with a log line,
/// so one broken bot does not hide the others.
pub fn list(bots_dir: &Path) -> Result<Vec<BotSummary>, BotError> {
    let entries = match fs::read_dir(bots_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut bots = Vec::new();
    for entry in entries {
        let entry = entry?;
        let Some(id) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !valid_id(&id) || !entry.path().is_dir() {
            continue;
        }
        match read_settings(&entry.path().join(SETTINGS_FILE)) {
            Ok(settings) => bots.push(summary(id, settings)),
            Err(error) => log::warn!("bot {id} is left out of the list: {error}"),
        }
    }
    bots.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
    Ok(bots)
}

/// Makes a bot's folder and its two files. A name already taken gets the next free folder, so no
/// existing bot is overwritten.
pub fn create(bots_dir: &Path, bot: NewBot) -> Result<BotSummary, BotError> {
    let name = bot.name.trim().to_owned();
    if name.is_empty() || name.chars().count() > MAX_NAME {
        return Err(BotError::InvalidName);
    }
    if !COLORS.contains(&bot.color.as_str()) {
        return Err(BotError::InvalidColor);
    }
    if !PROVIDERS.contains(&bot.provider.as_str()) {
        return Err(BotError::InvalidProvider);
    }
    check_instructions(&bot.instructions)?;
    let team = team_of(bot.team)?;
    let model = model_of(bot.model)?;
    let base = slug(&name);
    if base.is_empty() {
        return Err(BotError::InvalidName);
    }

    fs::create_dir_all(bots_dir)?;
    let (id, dir) = claim_folder(bots_dir, &base)?;
    let instructions = if bot.instructions.trim().is_empty() {
        format!("Você é {name}.")
    } else {
        bot.instructions
    };
    fs::write(dir.join(INSTRUCTIONS_FILE), instructions)?;
    let settings = BotSettings {
        name,
        color: bot.color,
        provider: bot.provider,
        model,
        team,
        repos: Vec::new(),
        commands: Vec::new(),
        workers: Vec::new(),
    };
    let text = serde_json::to_string_pretty(&settings).map_err(io::Error::other)?;
    fs::write(dir.join(SETTINGS_FILE), text)?;
    Ok(summary(id, settings))
}

/// The instructions a bot is given: something to say, and no more than the limit.
fn check_instructions(instructions: &str) -> Result<(), BotError> {
    let length = instructions.trim().chars().count();
    if length == 0 || length > MAX_INSTRUCTIONS {
        return Err(BotError::InvalidInstructions);
    }
    Ok(())
}

/// Replaces a bot's instructions. The next message the bot answers already uses them.
pub fn update_instructions(
    bots_dir: &Path,
    id: &str,
    instructions: &str,
) -> Result<BotDetail, BotError> {
    check_instructions(instructions)?;
    let folder = BotFolder::existing(bots_dir, id)?;
    private_file::write(
        &folder.dir().join(INSTRUCTIONS_FILE),
        instructions.trim().as_bytes(),
    )?;
    detail(bots_dir, id)
}

/// Changes a bot's engine, model and team. Its name, colour and instructions stay as they are.
pub fn update_setup(bots_dir: &Path, id: &str, setup: BotSetup) -> Result<BotDetail, BotError> {
    if !PROVIDERS.contains(&setup.provider.as_str()) {
        return Err(BotError::InvalidProvider);
    }
    let model = model_of(setup.model)?;
    let team = team_of(setup.team)?;
    let folder = BotFolder::existing(bots_dir, id)?;
    let mut settings = folder.settings()?;
    settings.provider = setup.provider;
    settings.model = model;
    settings.team = team;
    let text = serde_json::to_string_pretty(&settings).map_err(io::Error::other)?;
    private_file::write(&folder.dir().join(SETTINGS_FILE), text.as_bytes())?;
    detail(bots_dir, id)
}

/// The folder of a bot that exists, for the features that keep their files in it. Never makes one.
pub fn folder_of(bots_dir: &Path, id: &str) -> Result<PathBuf, BotError> {
    Ok(BotFolder::existing(bots_dir, id)?.dir().to_path_buf())
}

/// The repositories a bot may read, as the person granted them.
pub fn grants(bots_dir: &Path, id: &str) -> Result<Vec<Grant>, BotError> {
    Ok(BotFolder::existing(bots_dir, id)?.settings()?.repos)
}

/// Lets a bot read a repository. Granting one it already has changes nothing.
pub fn grant(bots_dir: &Path, id: &str, grant: Grant) -> Result<(), BotError> {
    edit_grants(bots_dir, id, |repos| {
        if !repos.iter().any(|known| known.id == grant.id) {
            repos.push(grant);
        }
    })
}

/// Takes back a repository a bot could read. A repository it never had changes nothing.
pub fn revoke(bots_dir: &Path, id: &str, repo_id: &str) -> Result<(), BotError> {
    edit_grants(bots_dir, id, |repos| {
        repos.retain(|known| known.id != repo_id)
    })
}

/// The repositories where a bot may start agents.
pub fn worker_grants(bots_dir: &Path, id: &str) -> Result<Vec<Grant>, BotError> {
    Ok(BotFolder::existing(bots_dir, id)?.settings()?.workers)
}

/// Lets a bot start agents in a repository. Granting one it already has changes nothing.
pub fn grant_workers(bots_dir: &Path, id: &str, grant: Grant) -> Result<(), BotError> {
    edit_settings(bots_dir, id, |settings| {
        if !settings.workers.iter().any(|known| known.id == grant.id) {
            settings.workers.push(grant);
        }
    })
}

/// Takes back the right to start agents in a repository.
pub fn revoke_workers(bots_dir: &Path, id: &str, repo_id: &str) -> Result<(), BotError> {
    edit_settings(bots_dir, id, |settings| {
        settings.workers.retain(|known| known.id != repo_id);
    })
}

fn edit_grants(
    bots_dir: &Path,
    id: &str,
    change: impl FnOnce(&mut Vec<Grant>),
) -> Result<(), BotError> {
    edit_settings(bots_dir, id, |settings| change(&mut settings.repos))
}

/// Changes a bot's `bot.json` in one step and keeps the rest of it as it was.
pub(super) fn edit_settings(
    bots_dir: &Path,
    id: &str,
    change: impl FnOnce(&mut BotSettings),
) -> Result<(), BotError> {
    let folder = BotFolder::existing(bots_dir, id)?;
    let mut settings = folder.settings()?;
    change(&mut settings);
    let text = serde_json::to_string_pretty(&settings).map_err(io::Error::other)?;
    private_file::write(&folder.dir().join(SETTINGS_FILE), text.as_bytes())?;
    Ok(())
}

/// Removes a bot's folder with its settings, instructions and conversation. Nothing outside it is touched.
pub fn delete(bots_dir: &Path, id: &str) -> Result<(), BotError> {
    let folder = BotFolder::existing(bots_dir, id)?;
    fs::remove_dir_all(folder.dir())?;
    Ok(())
}

/// One bot for the details tab: its settings, the instructions the user wrote, and where the folder is.
pub fn detail(bots_dir: &Path, id: &str) -> Result<BotDetail, BotError> {
    let folder = BotFolder::existing(bots_dir, id)?;
    let settings = folder.settings()?;
    let instructions = folder.instructions()?;
    let folder_path = folder.dir().display().to_string();
    let BotSummary {
        id,
        name,
        color,
        provider,
        model,
        team,
    } = summary(id.to_owned(), settings);
    Ok(BotDetail {
        id,
        name,
        color,
        provider,
        model,
        instructions,
        folder: folder_path,
        team,
    })
}

fn summary(id: String, settings: BotSettings) -> BotSummary {
    let name = if settings.name.trim().is_empty() {
        id.clone()
    } else {
        settings.name
    };
    BotSummary {
        id,
        name,
        color: settings.color,
        provider: settings.provider,
        model: settings.model,
        team: settings.team,
    }
}

/// The model a bot names, trimmed; a blank one means the provider's default.
fn model_of(model: Option<String>) -> Result<Option<String>, BotError> {
    let model = model
        .map(|model| model.trim().to_owned())
        .filter(|model| !model.is_empty());
    if model
        .as_deref()
        .is_some_and(|model| model.chars().count() > MAX_MODEL)
    {
        return Err(BotError::InvalidModel);
    }
    Ok(model)
}

/// The team a bot is given, trimmed; a blank one means no team.
fn team_of(team: Option<String>) -> Result<Option<String>, BotError> {
    let team = team
        .map(|team| team.trim().to_owned())
        .filter(|team| !team.is_empty());
    if team
        .as_deref()
        .is_some_and(|team| team.chars().count() > MAX_TEAM)
    {
        return Err(BotError::InvalidTeam);
    }
    Ok(team)
}

/// Makes the folder for `base`, or the first free `base-2`, `base-3`, … when it is taken.
fn claim_folder(bots_dir: &Path, base: &str) -> io::Result<(String, PathBuf)> {
    for number in 1..=999 {
        let id = if number == 1 {
            base.to_owned()
        } else {
            format!("{base}-{number}")
        };
        let dir = bots_dir.join(&id);
        match fs::create_dir(&dir) {
            Ok(()) => return Ok((id, dir)),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        ErrorKind::AlreadyExists,
        "no free folder name for this bot",
    ))
}

/// A folder name made from the bot's name: lowercase letters and digits, dashes between words.
pub fn slug(name: &str) -> String {
    let mut slug = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    slug.trim_end_matches('-').chars().take(MAX_NAME).collect()
}

fn read_settings(path: &Path) -> io::Result<BotSettings> {
    let text = fs::read_to_string(path)?;
    serde_json::from_str(&text).map_err(io::Error::other)
}

/// Lowercase letters, digits and dashes. A folder name must not reach outside `bots/`.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn write_if_missing(path: &Path, contents: &str) -> io::Result<()> {
    if path.exists() {
        return Ok(());
    }
    fs::write(path, contents)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_bot(name: &str) -> NewBot {
        NewBot {
            name: name.to_owned(),
            color: "sky".to_owned(),
            provider: "claude".to_owned(),
            model: None,
            instructions: "Cuide da agenda.".to_owned(),
            team: None,
        }
    }

    #[test]
    fn a_team_is_trimmed_and_a_blank_one_means_none() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut with_team = new_bot("Bento");
        with_team.team = Some("  infra ".to_owned());
        let created = create(dir.path(), with_team).expect("create");
        let mut blank = new_bot("Dev");
        blank.team = Some("   ".to_owned());
        let no_team = create(dir.path(), blank).expect("create");

        assert_eq!(created.team.as_deref(), Some("infra"));
        assert_eq!(no_team.team, None);
        assert_eq!(
            detail(dir.path(), &created.id)
                .expect("detail")
                .team
                .as_deref(),
            Some("infra")
        );
    }

    #[test]
    fn a_team_longer_than_24_characters_is_refused() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut bot = new_bot("Bento");
        bot.team = Some("a".repeat(25));

        assert!(matches!(
            create(dir.path(), bot),
            Err(BotError::InvalidTeam)
        ));
    }

    #[test]
    fn the_setup_changes_engine_model_and_team_and_keeps_the_rest() {
        let dir = tempfile::tempdir().expect("temp dir");
        let created = create(dir.path(), new_bot("Bento")).expect("create");

        let updated = update_setup(
            dir.path(),
            &created.id,
            BotSetup {
                provider: "claude".to_owned(),
                model: Some("  claude-haiku-5-5 ".to_owned()),
                team: Some(" dados ".to_owned()),
            },
        )
        .expect("update");

        assert_eq!(updated.model.as_deref(), Some("claude-haiku-5-5"));
        assert_eq!(updated.team.as_deref(), Some("dados"));
        assert_eq!(updated.name, "Bento");
        assert_eq!(updated.instructions, "Cuide da agenda.");
        assert!(matches!(
            update_setup(
                dir.path(),
                &created.id,
                BotSetup {
                    provider: "gemini".to_owned(),
                    model: None,
                    team: None,
                },
            ),
            Err(BotError::InvalidProvider)
        ));
    }

    #[test]
    fn a_new_folder_gets_the_default_files() {
        let dir = tempfile::tempdir().expect("temp dir");

        let folder = BotFolder::open(dir.path(), DEFAULT_BOT).expect("open");

        assert_eq!(
            folder.instructions().expect("instructions"),
            DEFAULT_INSTRUCTIONS
        );
        assert_eq!(folder.settings().expect("settings"), BotSettings::default());
    }

    #[test]
    fn what_the_user_wrote_is_kept_when_the_folder_opens_again() {
        let dir = tempfile::tempdir().expect("temp dir");
        let folder = BotFolder::open(dir.path(), DEFAULT_BOT).expect("open");
        fs::write(folder.dir().join(INSTRUCTIONS_FILE), "Minhas regras").expect("write");

        let again = BotFolder::open(dir.path(), DEFAULT_BOT).expect("reopen");

        assert_eq!(again.instructions().expect("instructions"), "Minhas regras");
    }

    #[test]
    fn a_bot_id_cannot_reach_outside_the_bots_folder() {
        let dir = tempfile::tempdir().expect("temp dir");

        for id in ["", "../escape", "Ada", "a/b", "a b"] {
            assert!(
                BotFolder::open(dir.path(), id).is_err(),
                "{id:?} should be refused"
            );
        }
    }

    #[test]
    fn a_created_bot_is_listed_with_its_files() {
        let dir = tempfile::tempdir().expect("temp dir");

        let created = create(dir.path(), new_bot("Ada Lovelace")).expect("create");

        assert_eq!(created.id, "ada-lovelace");
        assert_eq!(created.name, "Ada Lovelace");
        let listed = list(dir.path()).expect("list");
        assert_eq!(listed, vec![created]);
        let folder = BotFolder::open(dir.path(), "ada-lovelace").expect("open");
        assert_eq!(
            folder.instructions().expect("instructions"),
            "Cuide da agenda."
        );
    }

    #[test]
    fn a_name_taken_twice_gets_its_own_folder() {
        let dir = tempfile::tempdir().expect("temp dir");

        let first = create(dir.path(), new_bot("Bento")).expect("first");
        let second = create(dir.path(), new_bot("Bento")).expect("second");

        assert_eq!(first.id, "bento");
        assert_eq!(second.id, "bento-2");
        assert_eq!(list(dir.path()).expect("list").len(), 2);
    }

    #[test]
    fn a_name_without_letters_or_digits_is_refused() {
        let dir = tempfile::tempdir().expect("temp dir");

        assert!(matches!(
            create(dir.path(), new_bot("  ")),
            Err(BotError::InvalidName)
        ));
        assert!(matches!(
            create(dir.path(), new_bot("!!!")),
            Err(BotError::InvalidName)
        ));
    }

    #[test]
    fn an_unknown_colour_or_provider_is_refused() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut colour = new_bot("Bento");
        colour.color = "neon".to_owned();
        let mut provider = new_bot("Bento");
        provider.provider = "unknown".to_owned();

        assert!(matches!(
            create(dir.path(), colour),
            Err(BotError::InvalidColor)
        ));
        assert!(matches!(
            create(dir.path(), provider),
            Err(BotError::InvalidProvider)
        ));
    }

    #[test]
    fn a_broken_bot_is_left_out_and_the_others_stay() {
        let dir = tempfile::tempdir().expect("temp dir");
        create(dir.path(), new_bot("Bento")).expect("create");
        fs::create_dir(dir.path().join("quebrado")).expect("folder");
        fs::write(
            dir.path().join("quebrado").join(SETTINGS_FILE),
            "não é json",
        )
        .expect("write");

        let listed = list(dir.path()).expect("list");

        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "bento");
    }

    #[test]
    fn new_instructions_replace_the_old_ones_and_blank_ones_are_refused() {
        let dir = tempfile::tempdir().expect("temp dir");
        let created = create(dir.path(), new_bot("Bento")).expect("create");

        let updated =
            update_instructions(dir.path(), &created.id, "  Organize o dia.  ").expect("update");

        assert_eq!(updated.instructions, "Organize o dia.");
        assert!(matches!(
            update_instructions(dir.path(), &created.id, "   "),
            Err(BotError::InvalidInstructions)
        ));
    }

    #[test]
    fn a_bot_is_deleted_with_its_folder_and_an_unknown_one_is_refused() {
        let dir = tempfile::tempdir().expect("temp dir");
        let created = create(dir.path(), new_bot("Bento")).expect("create");

        delete(dir.path(), &created.id).expect("delete");

        assert!(!dir.path().join(&created.id).exists());
        assert!(matches!(
            delete(dir.path(), &created.id),
            Err(BotError::UnknownBot)
        ));
    }
}
