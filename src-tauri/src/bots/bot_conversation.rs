//! A conversation kept in a folder as `conversation.json`: what was written and what was answered,
//! oldest first. A bot's own chat and a channel use the same file format.

use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

use super::bot_access::AccessRequest;
use super::bot_folder::BotError;
use crate::private_file;

const CONVERSATION_FILE: &str = "conversation.json";
/// The longest message the user may send, in characters.
pub const MAX_MESSAGE: usize = 4000;
/// How much of the earlier conversation goes back to the CLI with each message. Command lines are
/// capped, and the newest turns matter most.
const MAX_CONTEXT: usize = 12_000;

/// Who wrote a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum Author {
    User,
    Bot,
    /// A reply that did not come: kept for the user to see, never sent back to the engine.
    Error,
}

/// One message of a conversation. `at` is when it was written, in seconds since the Unix epoch.
/// `speaker` is the bot that wrote a message in a channel; a bot's own chat leaves it empty.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub id: String,
    pub author: Author,
    pub text: String,
    pub at: u32,
    #[serde(default)]
    pub speaker: Option<String>,
    /// Set on a message that asks the person for access to a repository.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<AccessRequest>,
    /// The person stopped this answer: its text is what had been written until then.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub stopped: bool,
}

impl ChatMessage {
    pub fn new(author: Author, text: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            author,
            text: text.to_owned(),
            at: now_seconds(),
            speaker: None,
            request: None,
            stopped: false,
        }
    }

    pub fn spoken_by(mut self, speaker: &str) -> Self {
        self.speaker = Some(speaker.to_owned());
        self
    }

    /// Marks the message as an answer the person stopped.
    pub fn stopped(mut self) -> Self {
        self.stopped = true;
        self
    }

    pub fn requesting(mut self, request: AccessRequest) -> Self {
        self.request = Some(request);
        self
    }
}

fn now_seconds() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| u32::try_from(elapsed.as_secs()).ok())
        .unwrap_or(0)
}

/// Every message so far, oldest first. A folder with no conversation yet has none.
pub fn read(dir: &Path) -> Result<Vec<ChatMessage>, BotError> {
    match fs::read_to_string(dir.join(CONVERSATION_FILE)) {
        Ok(text) => Ok(serde_json::from_str(&text)?),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error.into()),
    }
}

/// Adds messages to the end. A file that does not parse is an error, and is left as it is.
pub fn append(dir: &Path, messages: &[ChatMessage]) -> Result<(), BotError> {
    let mut all = read(dir)?;
    all.extend_from_slice(messages);
    write(dir, &all)
}

/// Writes the whole conversation again, for a message that changed, such as an answered request.
pub fn write(dir: &Path, messages: &[ChatMessage]) -> Result<(), BotError> {
    let text = serde_json::to_string_pretty(messages)?;
    private_file::write(&dir.join(CONVERSATION_FILE), text.as_bytes())?;
    Ok(())
}

/// Adds a message right after the one with id `after`, so a reply sits under the message it answers
/// even when the bot saved other messages while it wrote it. At the end when `after` is not there.
pub fn insert_after(dir: &Path, after: &str, message: &ChatMessage) -> Result<(), BotError> {
    let mut all = read(dir)?;
    let at = all
        .iter()
        .position(|turn| turn.id == after)
        .map_or(all.len(), |index| index + 1);
    all.insert(at, message.clone());
    write(dir, &all)
}

/// What the CLI is asked in a bot's own chat: the conversation, the pessoa and the bot named in it.
pub fn prompt(history: &[ChatMessage], bot_name: &str, message: &str) -> String {
    let text = lines(history, &|turn| match turn.author {
        Author::User => "Pessoa".to_owned(),
        _ => bot_name.to_owned(),
    });
    if text.is_empty() {
        return message.to_owned();
    }
    format!(
        "Conversa até agora, da mais antiga para a mais nova:\n{text}\n\nÚltima mensagem da pessoa, a que você responde:\n{message}"
    )
}

/// The conversation, one line per message labelled by `label`, as much of its newest end as the
/// context allows. Replies that did not come are left out.
pub fn lines(history: &[ChatMessage], label: &dyn Fn(&ChatMessage) -> String) -> String {
    let mut kept = Vec::new();
    let mut size = 0;
    for turn in history
        .iter()
        .rev()
        .filter(|turn| turn.author != Author::Error)
    {
        let line = format!("{}: {}", label(turn), turn.text);
        size += line.chars().count();
        if size > MAX_CONTEXT {
            break;
        }
        kept.push(line);
    }
    kept.reverse();
    kept.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bots::bot_folder::BotFolder;
    use tempfile::TempDir;

    fn bot_folder(dir: &TempDir) -> BotFolder {
        BotFolder::open(&dir.path().join("bots"), "ada").expect("the folder opens")
    }

    #[test]
    fn a_new_bot_has_no_conversation() {
        let dir = TempDir::new().expect("a temporary folder");
        assert!(read(bot_folder(&dir).dir()).expect("reads").is_empty());
    }

    #[test]
    fn messages_are_kept_in_the_order_they_were_added() {
        let dir = TempDir::new().expect("a temporary folder");
        let folder = bot_folder(&dir);
        let first = ChatMessage::new(Author::User, "oi");
        let second = ChatMessage::new(Author::Bot, "oi, tudo bem?");
        append(folder.dir(), std::slice::from_ref(&first)).expect("first is kept");
        append(folder.dir(), std::slice::from_ref(&second)).expect("second is kept");
        assert_eq!(read(folder.dir()).expect("reads"), vec![first, second]);
    }

    #[test]
    fn a_broken_file_is_an_error_and_is_not_overwritten() {
        let dir = TempDir::new().expect("a temporary folder");
        let folder = bot_folder(&dir);
        let path = folder.dir().join(CONVERSATION_FILE);
        fs::write(&path, "não é json").expect("the file is written");
        let message = ChatMessage::new(Author::User, "oi");
        assert!(matches!(
            append(folder.dir(), &[message]),
            Err(BotError::Conversation(_))
        ));
        assert_eq!(fs::read_to_string(&path).expect("reads"), "não é json");
    }

    #[test]
    fn without_history_the_prompt_is_the_message_alone() {
        assert_eq!(prompt(&[], "Ada", "bom dia"), "bom dia");
    }

    #[test]
    fn a_long_history_keeps_its_newest_turns() {
        let history = [
            ChatMessage::new(Author::User, &format!("primeiro {}", "a".repeat(7000))),
            ChatMessage::new(Author::Bot, &format!("segundo {}", "b".repeat(7000))),
            ChatMessage::new(Author::User, "terceiro"),
        ];
        let text = prompt(&history, "Ada", "quarto");
        assert!(!text.contains("primeiro"));
        assert!(text.contains("Ada: segundo"));
        assert!(text.contains("Pessoa: terceiro"));
        assert!(text.ends_with("quarto"));
    }

    #[test]
    fn an_error_is_kept_but_not_sent_back_to_the_engine() {
        let history = [
            ChatMessage::new(Author::User, "oi"),
            ChatMessage::new(Author::Error, "Claude did not answer."),
            ChatMessage::new(Author::User, "de novo"),
        ];

        let text = prompt(&history, "Ada", "agora");

        assert!(!text.contains("did not answer"));
        assert!(text.contains("Pessoa: de novo"));
    }

    #[test]
    fn a_reply_lands_under_the_message_it_answers_even_after_a_request_was_saved() {
        let dir = TempDir::new().expect("a temporary folder");
        let question = ChatMessage::new(Author::User, "oi");
        let request = ChatMessage::new(Author::Bot, "Preciso ler o projeto.");
        append(dir.path(), &[question.clone(), request]).expect("saved");
        let reply = ChatMessage::new(Author::Bot, "Pronto, pedi acesso.");

        insert_after(dir.path(), &question.id, &reply).expect("inserted");

        let texts: Vec<String> = read(dir.path())
            .expect("read")
            .into_iter()
            .map(|turn| turn.text)
            .collect();
        assert_eq!(
            texts,
            ["oi", "Pronto, pedi acesso.", "Preciso ler o projeto."]
        );
    }
}
