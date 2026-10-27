//! A bot's threads: separate conversations with the same bot. Each lives in
//! `bots/<id>/threads/<thread>/`, with its own `conversation.json` and a `thread.json` holding its
//! title. What belongs to the bot (its instructions, memory, skills, commands) is shared by all of
//! its threads. A thread with no title yet shows as untitled, and takes its title from its first message.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use specta::Type;

use super::bot_folder::BotError;
use crate::private_file;

const THREADS_DIR: &str = "threads";
const META_FILE: &str = "thread.json";
const CONVERSATION_FILE: &str = "conversation.json";
const MAX_TITLE: usize = 60;
/// How much of a first message becomes the title.
const TITLE_FROM_MESSAGE: usize = 40;

/// A thread as the screen lists it. Times are Unix seconds; `title` is empty until it has one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ThreadSummary {
    pub id: String,
    pub title: String,
    pub created_at: u32,
    pub updated_at: u32,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct Meta {
    title: String,
    created_at: u32,
}

/// The key a thread's events and conversation go by on the screen.
pub fn key_of(bot: &str, thread: &str) -> String {
    format!("bot:{bot}:{thread}")
}

/// The folder of a thread that exists. An id that is not one of ours never reaches outside `threads/`.
pub fn dir(bot_dir: &Path, thread: &str) -> Result<PathBuf, BotError> {
    if !valid_id(thread) {
        return Err(BotError::UnknownThread);
    }
    let dir = bot_dir.join(THREADS_DIR).join(thread);
    if !dir.join(META_FILE).is_file() {
        return Err(BotError::UnknownThread);
    }
    Ok(dir)
}

/// The bot's threads, the most recently active first. A bot with none gets its first one, so a chat
/// always has somewhere to write.
pub fn list(bot_dir: &Path) -> Result<Vec<ThreadSummary>, BotError> {
    let mut threads = read_all(bot_dir);
    if threads.is_empty() {
        threads.push(create(bot_dir)?);
    }
    threads.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then(b.created_at.cmp(&a.created_at))
    });
    Ok(threads)
}

/// Starts a new, untitled thread.
pub fn create(bot_dir: &Path) -> Result<ThreadSummary, BotError> {
    let id = uuid::Uuid::new_v4().simple().to_string();
    let dir = bot_dir.join(THREADS_DIR).join(&id);
    fs::create_dir_all(&dir)?;
    let now = now();
    write_meta(
        &dir,
        &Meta {
            title: String::new(),
            created_at: now,
        },
    )?;
    Ok(ThreadSummary {
        id,
        title: String::new(),
        created_at: now,
        updated_at: now,
    })
}

/// Gives a thread a title. A blank one makes it untitled again.
pub fn rename(bot_dir: &Path, thread: &str, title: &str) -> Result<ThreadSummary, BotError> {
    let title = title.trim();
    if title.chars().count() > MAX_TITLE {
        return Err(BotError::InvalidThreadTitle);
    }
    let dir = dir(bot_dir, thread)?;
    let mut meta = read_meta(&dir).ok_or(BotError::UnknownThread)?;
    title.clone_into(&mut meta.title);
    write_meta(&dir, &meta)?;
    summary(thread, &dir).ok_or(BotError::UnknownThread)
}

/// Titles a thread from its first message, once. A thread the person already titled keeps its title.
pub fn title_from_first_message(bot_dir: &Path, thread: &str, message: &str) {
    let Ok(dir) = dir(bot_dir, thread) else {
        return;
    };
    let Some(mut meta) = read_meta(&dir) else {
        return;
    };
    if !meta.title.is_empty() {
        return;
    }
    let first = message
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default();
    let words = first.split_whitespace().collect::<Vec<_>>().join(" ");
    let cut: String = words.chars().take(TITLE_FROM_MESSAGE).collect();
    meta.title = if words.chars().count() > TITLE_FROM_MESSAGE {
        format!("{}…", cut.trim_end())
    } else {
        cut
    };
    let _ = write_meta(&dir, &meta);
}

/// Removes a thread with its conversation. The bot's other threads are untouched.
pub fn delete(bot_dir: &Path, thread: &str) -> Result<(), BotError> {
    fs::remove_dir_all(dir(bot_dir, thread)?)?;
    Ok(())
}

/// The thread that was active last, for what the bot does on its own: it answers there.
pub fn latest(bot_dir: &Path) -> Result<String, BotError> {
    Ok(list(bot_dir)?.remove(0).id)
}

fn read_all(bot_dir: &Path) -> Vec<ThreadSummary> {
    let Ok(entries) = fs::read_dir(bot_dir.join(THREADS_DIR)) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let id = entry.file_name().to_string_lossy().into_owned();
            if valid_id(&id) {
                summary(&id, &entry.path())
            } else {
                None
            }
        })
        .collect()
}

fn summary(id: &str, dir: &Path) -> Option<ThreadSummary> {
    let meta = read_meta(dir)?;
    let updated = fs::metadata(dir.join(CONVERSATION_FILE))
        .and_then(|file| file.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .and_then(|elapsed| u32::try_from(elapsed.as_secs()).ok())
        .unwrap_or(meta.created_at);
    Some(ThreadSummary {
        id: id.to_owned(),
        title: meta.title,
        created_at: meta.created_at,
        updated_at: updated.max(meta.created_at),
    })
}

fn read_meta(dir: &Path) -> Option<Meta> {
    serde_json::from_str(&fs::read_to_string(dir.join(META_FILE)).ok()?).ok()
}

fn write_meta(dir: &Path, meta: &Meta) -> io::Result<()> {
    let text = serde_json::to_string_pretty(meta).map_err(io::Error::other)?;
    private_file::write(&dir.join(META_FILE), text.as_bytes())
}

/// 32 lowercase hex digits: what `create` makes, and nothing that could name another folder.
fn valid_id(id: &str) -> bool {
    id.len() == 32 && id.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'))
}

fn now() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u32::try_from(elapsed.as_secs()).unwrap_or(u32::MAX)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bot() -> tempfile::TempDir {
        tempfile::tempdir().expect("temp dir")
    }

    #[test]
    fn a_bot_with_no_thread_gets_its_first_one_untitled() {
        let dir = bot();

        let listed = list(dir.path()).expect("list");

        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].title, "");
        assert_eq!(
            list(dir.path()).expect("again"),
            listed,
            "asking twice makes no second thread"
        );
    }

    #[test]
    fn threads_are_separate_folders_and_the_newest_activity_comes_first() {
        let dir = bot();
        let first = create(dir.path()).expect("first");
        let second = create(dir.path()).expect("second");
        assert_ne!(first.id, second.id);
        assert_ne!(
            super::dir(dir.path(), &first.id).expect("dir"),
            super::dir(dir.path(), &second.id).expect("dir")
        );

        // The conversation file's time is what orders them.
        let touched = super::dir(dir.path(), &first.id)
            .expect("dir")
            .join(CONVERSATION_FILE);
        fs::write(&touched, "[]").expect("write");
        let file = fs::File::options()
            .write(true)
            .open(&touched)
            .expect("open");
        file.set_modified(SystemTime::now() + std::time::Duration::from_secs(60))
            .expect("later");

        assert_eq!(list(dir.path()).expect("list")[0].id, first.id);
        assert_eq!(latest(dir.path()).expect("latest"), first.id);
    }

    #[test]
    fn a_thread_is_titled_from_its_first_message_once_and_can_be_renamed() {
        let dir = bot();
        let thread = create(dir.path()).expect("thread");

        title_from_first_message(
            dir.path(),
            &thread.id,
            "\n  Revisa o PR   do botão de copiar, por favor, e me diz o que achou",
        );
        let titled = list(dir.path()).expect("list")[0].clone();
        assert_eq!(titled.title, "Revisa o PR do botão de copiar, por favo…");

        title_from_first_message(dir.path(), &thread.id, "outra coisa");
        assert_eq!(
            list(dir.path()).expect("list")[0].title,
            titled.title,
            "only the first message titles it"
        );

        assert_eq!(
            rename(dir.path(), &thread.id, "  Revisão  ")
                .expect("renamed")
                .title,
            "Revisão"
        );
        assert_eq!(
            rename(dir.path(), &thread.id, "").expect("cleared").title,
            ""
        );
        assert!(matches!(
            rename(dir.path(), &thread.id, &"x".repeat(61)),
            Err(BotError::InvalidThreadTitle)
        ));
    }

    #[test]
    fn deleting_a_thread_leaves_the_others_and_an_id_cannot_reach_outside() {
        let dir = bot();
        let keep = create(dir.path()).expect("keep");
        let drop = create(dir.path()).expect("drop");

        delete(dir.path(), &drop.id).expect("deleted");

        let ids: Vec<String> = list(dir.path())
            .expect("list")
            .into_iter()
            .map(|thread| thread.id)
            .collect();
        assert_eq!(ids, [keep.id]);
        for bad in ["..", "../..", "", "nao-e-um-id", &"A".repeat(32)] {
            assert!(
                matches!(super::dir(dir.path(), bad), Err(BotError::UnknownThread)),
                "{bad:?}"
            );
            assert!(delete(dir.path(), bad).is_err());
        }
    }
}
