//! A bot's topic notes: longer notes about one subject in `memory/<topic>.md`, which the person can
//! open and edit. They never load by themselves: the prompt lists their names, recall brings up the
//! passages that match a message, and the bot reads or writes one with a tool. A file may start with
//! `---` frontmatter holding `title`, `description` and `aliases`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use super::bot_folder::BotError;
use super::bot_memory;
use crate::private_file;

const DIR: &str = "memory";
/// The archive lives next to the topics but is not one.
const RESERVED: &str = "archive";
const MAX_NAME: usize = 48;
const MAX_INDEXED: usize = 40;
const MAX_INDEX_CHARS: usize = 2000;
const MAX_TOPIC_BYTES: usize = 64 * 1024;
/// Keeps two writes from reading the same file and losing one of them.
static WRITING: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Topic {
    pub name: String,
    pub title: String,
    pub description: String,
    pub aliases: Vec<String>,
}

/// Letters, digits and dashes: a name can never leave `memory/`.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_NAME
        && name != RESERVED
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// A free-form subject as a topic name, or None when nothing usable is left.
pub fn name_from(subject: &str) -> Option<String> {
    let slug: String = subject
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let slug: String = slug.chars().take(MAX_NAME).collect();
    let slug = slug.trim_matches('-').to_owned();
    valid_name(&slug).then_some(slug)
}

fn file_of(bot_dir: &Path, name: &str) -> Result<PathBuf, BotError> {
    if !valid_name(name) {
        return Err(BotError::InvalidMemory(
            "a topic name has lowercase letters, digits and dashes".to_owned(),
        ));
    }
    Ok(bot_dir.join(DIR).join(format!("{name}.md")))
}

/// The topics, by name.
pub fn list(bot_dir: &Path) -> Vec<Topic> {
    let Ok(entries) = fs::read_dir(bot_dir.join(DIR)) else {
        return Vec::new();
    };
    let mut topics: Vec<Topic> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let file = entry.file_name().to_string_lossy().into_owned();
            let name = file.strip_suffix(".md")?;
            if !valid_name(name) || !entry.path().is_file() {
                return None;
            }
            let text = fs::read_to_string(entry.path()).ok()?;
            Some(topic_of(name, &text))
        })
        .collect();
    topics.sort_by(|a, b| a.name.cmp(&b.name));
    topics
}

/// A topic's text, without its frontmatter.
pub fn body_of(text: &str) -> &str {
    split(text).1
}

/// The frontmatter fields and the rest of the file.
fn split(text: &str) -> (&str, &str) {
    let Some(rest) = text.strip_prefix("---\n") else {
        return ("", text);
    };
    match rest.split_once("\n---") {
        Some((front, after)) => (front, after.strip_prefix('\n').unwrap_or(after)),
        None => ("", text),
    }
}

fn topic_of(name: &str, text: &str) -> Topic {
    let field = |key: &str| {
        split(text)
            .0
            .lines()
            .find_map(|line| line.strip_prefix(key)?.strip_prefix(':'))
            .map(|value| value.trim().trim_matches(['[', ']']).to_owned())
            .unwrap_or_default()
    };
    let title = field("title");
    Topic {
        name: name.to_owned(),
        title: if title.is_empty() {
            name.to_owned()
        } else {
            title
        },
        description: field("description"),
        aliases: field("aliases")
            .split(',')
            .map(|alias| alias.trim().to_lowercase())
            .filter(|alias| !alias.is_empty())
            .collect(),
    }
}

/// The part of the prompt that names the topics, or None when there are none.
pub fn index_section(bot_dir: &Path) -> Option<String> {
    let topics = list(bot_dir);
    if topics.is_empty() {
        return None;
    }
    let mut text = String::from(
        "Seus tópicos de memória (notas longas, que não carregam sozinhas; leia um com bot_memory_topic quando precisar):\n",
    );
    for topic in topics.iter().take(MAX_INDEXED) {
        let line = if topic.description.is_empty() {
            format!("- {}: {}\n", topic.name, topic.title)
        } else {
            format!(
                "- {}: {} — {}\n",
                topic.name, topic.title, topic.description
            )
        };
        if text.len() + line.len() > MAX_INDEX_CHARS {
            break;
        }
        text.push_str(&line);
    }
    Some(text.trim_end().to_owned())
}

pub fn read(bot_dir: &Path, name: &str) -> Result<String, BotError> {
    fs::read_to_string(file_of(bot_dir, name)?)
        .map_err(|_| BotError::InvalidMemory(format!("there is no topic {name}")))
}

/// Adds a dated entry to a topic, creating the topic with a header when it is new.
pub fn append(
    bot_dir: &Path,
    name: &str,
    title: Option<&str>,
    aliases: Option<&str>,
    entry: &str,
    date: &str,
) -> Result<(), BotError> {
    let file = file_of(bot_dir, name)?;
    let lines = bot_memory::entry_lines(entry, date)?;
    let _guard = WRITING.lock().unwrap_or_else(PoisonError::into_inner);
    let mut text = fs::read_to_string(&file).unwrap_or_else(|_| {
        let title = bot_memory::redact(title.unwrap_or(name).trim());
        let aliases = bot_memory::redact(aliases.unwrap_or_default().trim());
        format!("---\ntitle: {title}\naliases: {aliases}\n---\n")
    });
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(&lines.join("\n"));
    text.push('\n');
    store(&file, &text)
}

/// Replaces a topic with `text`, with secrets replaced: the bot's way to correct one.
pub fn write(bot_dir: &Path, name: &str, text: &str) -> Result<(), BotError> {
    let file = file_of(bot_dir, name)?;
    let _guard = WRITING.lock().unwrap_or_else(PoisonError::into_inner);
    store(&file, &bot_memory::redact(text))
}

fn store(file: &Path, text: &str) -> Result<(), BotError> {
    if text.len() > MAX_TOPIC_BYTES {
        return Err(BotError::InvalidMemory(format!(
            "a topic can hold up to {} KB",
            MAX_TOPIC_BYTES / 1024
        )));
    }
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)?;
    }
    private_file::write(file, text.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_subject_becomes_a_safe_name_and_nothing_can_leave_the_folder() {
        assert_eq!(
            name_from("  Viagem p/ Lisboa! ").as_deref(),
            Some("viagem-p-lisboa")
        );
        assert_eq!(name_from("???"), None);
        assert_eq!(name_from("archive"), None);
        for bad in ["", "..", "a/b", "A", "archive", &"x".repeat(49)] {
            assert!(!valid_name(bad), "{bad:?}");
        }
        let dir = tempfile::tempdir().expect("dir");
        assert!(read(dir.path(), "../x").is_err());
        assert!(append(dir.path(), "../x", None, None, "nota", "2026-10-27").is_err());
    }

    #[test]
    fn a_new_topic_gets_a_header_and_entries_are_added_dated_without_secrets() {
        let dir = tempfile::tempdir().expect("dir");
        append(
            dir.path(),
            "lisboa",
            Some("Viagem a Lisboa"),
            Some("portugal, viagem"),
            "Voo dia 12.\nChave sk-abcdefghijklmnopqrstuvwxyz123456",
            "2026-10-27",
        )
        .expect("appended");
        append(
            dir.path(),
            "lisboa",
            None,
            None,
            "Hotel perto do Chiado",
            "2026-10-28",
        )
        .expect("again");

        let text = read(dir.path(), "lisboa").expect("read");
        assert!(text.starts_with("---\ntitle: Viagem a Lisboa\n"), "{text}");
        assert!(text.contains("- [2026-10-27] Voo dia 12.\n  Chave") && !text.contains("sk-abc"));
        assert!(text.contains("- [2026-10-28] Hotel"));
        assert!(body_of(&text).starts_with("- [2026-10-27]"), "{text}");

        let topics = list(dir.path());
        assert_eq!(topics.len(), 1);
        assert_eq!(topics[0].title, "Viagem a Lisboa");
        assert_eq!(topics[0].aliases, ["portugal", "viagem"]);
    }

    #[test]
    fn the_index_lists_topics_and_the_archive_is_not_one() {
        let dir = tempfile::tempdir().expect("dir");
        assert_eq!(index_section(dir.path()), None);
        write(
            dir.path(),
            "carro",
            "---\ntitle: Carro\ndescription: o Civic\n---\n- [2026-10-27] x\n",
        )
        .expect("written");
        fs::write(dir.path().join(DIR).join("archive.md"), "- velho").expect("archive");

        let index = index_section(dir.path()).expect("index");
        assert!(index.contains("- carro: Carro — o Civic"), "{index}");
        assert!(!index.contains("archive"), "{index}");
    }
}
