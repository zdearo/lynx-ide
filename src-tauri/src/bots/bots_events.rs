use serde::{Deserialize, Serialize};
use specta::Type;
use tauri_specta::Event;

use super::bot_conversation::ChatMessage;
use super::chatgpt_login::ChatGptStatus;

/// The ChatGPT sign-in changed: an attempt started, ended, or a sign-out finished.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct ChatGptAuthChanged {
    pub status: ChatGptStatus,
}

/// A piece of a reply, as its engine writes it. `thread` is the chat it belongs to (`bot:<id>` for a
/// bot's own chat, `channel:<id>` for a channel) and `author` the bot writing it.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct BotReplyDelta {
    pub thread: String,
    pub author: String,
    pub text: String,
}

/// Where a channel's answer stands: `routing` while the router picks who answers, `answering` while a
/// member writes (`author`), and `idle` once nothing is left. `waiting` are the members still to answer.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct ChannelStatus {
    pub thread: String,
    pub phase: String,
    pub author: Option<String>,
    pub waiting: Vec<String>,
}

/// A message was saved to a channel's conversation, so the chat shows it as soon as it is written.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessageAdded {
    pub thread: String,
    pub message: ChatMessage,
}
