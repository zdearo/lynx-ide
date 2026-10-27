//! A channel: a shared conversation between the person and a few bots. It lives in its own folder,
//! `LYNX_HOME/channels/<id>/`, with `channel.json` (its name and members) and the `conversation.json`
//! every chat keeps. Channels sit outside teams: a channel names its members, not a team.

use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use specta::Type;

use super::bot_folder::{self, BotError, BotFolder};
use crate::private_file;

const SETTINGS_FILE: &str = "channel.json";
const MAX_NAME: usize = 40;

/// What `channel.json` says. A file that lacks a field gets the default for it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ChannelSettings {
    pub name: String,
    pub members: Vec<String>,
}

/// A channel as the sidebar lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ChannelSummary {
    pub id: String,
    pub name: String,
    pub members: Vec<String>,
}

/// What the form asks for when a channel is made.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct NewChannel {
    pub name: String,
    pub members: Vec<String>,
}

/// Every channel in `channels_dir`, by name. A folder whose settings cannot be read is left out.
pub fn list(channels_dir: &Path) -> Result<Vec<ChannelSummary>, BotError> {
    let entries = match fs::read_dir(channels_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut channels = Vec::new();
    for entry in entries {
        let dir = entry?.path();
        let Some(id) = dir
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
        else {
            continue;
        };
        match read_settings(&dir) {
            Ok(settings) => channels.push(summary(id, settings)),
            Err(BotError::Io(error)) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => log::warn!("channel {id} is left out of the list: {error}"),
        }
    }
    channels.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
    Ok(channels)
}

/// Makes a channel's folder. Its members must be bots that exist, each named once.
pub fn create(
    channels_dir: &Path,
    bots_dir: &Path,
    channel: NewChannel,
) -> Result<ChannelSummary, BotError> {
    let name = channel.name.trim().to_owned();
    if name.is_empty()
        || name.chars().count() > MAX_NAME
        || !name.chars().any(char::is_alphanumeric)
    {
        return Err(BotError::InvalidChannel);
    }
    let id = bot_folder::slug(&name);
    if id.is_empty() {
        return Err(BotError::InvalidChannel);
    }
    let members = checked_members(bots_dir, channel.members)?;
    let dir = channels_dir.join(&id);
    if dir.exists() {
        return Err(BotError::ChannelExists);
    }
    fs::create_dir_all(&dir)?;
    write_settings(
        &dir,
        &ChannelSettings {
            name: name.clone(),
            members: members.clone(),
        },
    )?;
    Ok(ChannelSummary { id, name, members })
}

/// Replaces a channel's members. Its name and conversation stay as they are.
pub fn update_members(
    channels_dir: &Path,
    bots_dir: &Path,
    id: &str,
    members: Vec<String>,
) -> Result<ChannelSummary, BotError> {
    let (dir, settings) = open(channels_dir, id)?;
    let settings = ChannelSettings {
        members: checked_members(bots_dir, members)?,
        ..settings
    };
    write_settings(&dir, &settings)?;
    Ok(summary(id.to_owned(), settings))
}

/// Removes a channel's folder, with its settings and conversation. The bots in it stay as they are.
pub fn delete(channels_dir: &Path, id: &str) -> Result<(), BotError> {
    let (dir, _) = open(channels_dir, id)?;
    fs::remove_dir_all(dir)?;
    Ok(())
}

/// The members a channel names: each one a bot that exists, once.
fn checked_members(bots_dir: &Path, members: Vec<String>) -> Result<Vec<String>, BotError> {
    let mut checked: Vec<String> = Vec::new();
    for member in members {
        BotFolder::existing(bots_dir, &member)?;
        if !checked.contains(&member) {
            checked.push(member);
        }
    }
    Ok(checked)
}

fn write_settings(dir: &Path, settings: &ChannelSettings) -> Result<(), BotError> {
    let text = serde_json::to_string_pretty(settings).map_err(io::Error::other)?;
    private_file::write(&dir.join(SETTINGS_FILE), text.as_bytes())?;
    Ok(())
}

/// A channel's folder and settings, for a channel that exists.
pub fn open(channels_dir: &Path, id: &str) -> Result<(PathBuf, ChannelSettings), BotError> {
    if !bot_folder::valid_id(id) {
        return Err(BotError::UnknownChannel);
    }
    let dir = channels_dir.join(id);
    if !dir.join(SETTINGS_FILE).is_file() {
        return Err(BotError::UnknownChannel);
    }
    let settings = read_settings(&dir)?;
    Ok((dir, settings))
}

fn read_settings(dir: &Path) -> Result<ChannelSettings, BotError> {
    let text = fs::read_to_string(dir.join(SETTINGS_FILE))?;
    Ok(serde_json::from_str(&text).map_err(io::Error::other)?)
}

fn summary(id: String, settings: ChannelSettings) -> ChannelSummary {
    ChannelSummary {
        id,
        name: settings.name,
        members: settings.members,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bots::bot_folder::{NewBot, create as create_bot};

    fn bot(dir: &Path, name: &str) -> String {
        create_bot(
            dir,
            NewBot {
                name: name.to_owned(),
                color: "sky".to_owned(),
                provider: "claude".to_owned(),
                model: None,
                instructions: "Ajude.".to_owned(),
                team: None,
            },
        )
        .expect("the bot is made")
        .id
    }

    #[test]
    fn a_channel_is_listed_with_its_members_once_each() {
        let root = tempfile::tempdir().expect("temp dir");
        let (bots, channels) = (root.path().join("bots"), root.path().join("channels"));
        let ada = bot(&bots, "Ada");

        let made = create(
            &channels,
            &bots,
            NewChannel {
                name: " Engenharia ".to_owned(),
                members: vec![ada.clone(), ada.clone()],
            },
        )
        .expect("create");

        assert_eq!(made.id, "engenharia");
        assert_eq!(made.name, "Engenharia");
        assert_eq!(made.members, vec![ada]);
        assert_eq!(list(&channels).expect("list"), vec![made]);
    }

    #[test]
    fn the_members_change_and_the_name_stays() {
        let root = tempfile::tempdir().expect("temp dir");
        let (bots, channels) = (root.path().join("bots"), root.path().join("channels"));
        let ada = bot(&bots, "Ada");
        let bento = bot(&bots, "Bento");
        create(
            &channels,
            &bots,
            NewChannel {
                name: "Infra".to_owned(),
                members: vec![ada.clone()],
            },
        )
        .expect("create");

        let updated = update_members(
            &channels,
            &bots,
            "infra",
            vec![bento.clone(), bento.clone()],
        )
        .expect("update");

        assert_eq!(updated.name, "Infra");
        assert_eq!(updated.members, vec![bento]);
        assert!(matches!(
            update_members(&channels, &bots, "infra", vec!["ninguem".to_owned()]),
            Err(BotError::UnknownBot)
        ));
    }

    #[test]
    fn a_deleted_channel_goes_with_its_folder_and_its_bots_stay() {
        let root = tempfile::tempdir().expect("temp dir");
        let (bots, channels) = (root.path().join("bots"), root.path().join("channels"));
        let ada = bot(&bots, "Ada");
        create(
            &channels,
            &bots,
            NewChannel {
                name: "Infra".to_owned(),
                members: vec![ada.clone()],
            },
        )
        .expect("create");

        delete(&channels, "infra").expect("delete");

        assert!(list(&channels).expect("list").is_empty());
        assert!(!channels.join("infra").exists());
        assert!(bot_folder::detail(&bots, &ada).is_ok());
        assert!(matches!(
            delete(&channels, "infra"),
            Err(BotError::UnknownChannel)
        ));
    }

    #[test]
    fn a_member_that_is_not_a_bot_or_a_repeated_name_is_refused() {
        let root = tempfile::tempdir().expect("temp dir");
        let (bots, channels) = (root.path().join("bots"), root.path().join("channels"));
        bot(&bots, "Ada");
        let new = |name: &str, member: &str| NewChannel {
            name: name.to_owned(),
            members: vec![member.to_owned()],
        };

        assert!(matches!(
            create(&channels, &bots, new("Infra", "ninguem")),
            Err(BotError::UnknownBot)
        ));
        create(&channels, &bots, new("Infra", "ada")).expect("first");
        assert!(matches!(
            create(&channels, &bots, new("infra", "ada")),
            Err(BotError::ChannelExists)
        ));
        assert!(matches!(
            open(&channels, "nao-existe"),
            Err(BotError::UnknownChannel)
        ));
    }
}
