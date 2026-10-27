//! Talks to a bot: keeps its conversation in the bot's folder and asks its engine for each reply.
//! The conversation file is read and written under a lock, so two messages cannot overwrite each
//! other. The lock is never held while the engine answers; a bot that is answering is marked busy,
//! so it cannot be deleted under the answer.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

use chrono::Local;
use serde::{Deserialize, Serialize};
use specta::Type;

use super::bot_access::{AccessRequest, AccessState, Grant};
use super::bot_conversation::{self, Author, ChatMessage, MAX_MESSAGE};
use super::bot_folder::{self, BotError, BotFolder};
use super::bot_memory;
use super::bot_memory_capture;
use super::bot_recall;
use super::bot_thread::{self, ThreadSummary};
use super::bot_tools::Announce;
use super::bot_topics;
use super::claude_cli::{ClaudeAnswer, ClaudeCli};
use super::command_permissions;
use super::turn_note;

/// What asking for access did: the person is asked now, or was asked and has not answered yet.
pub enum RequestOutcome {
    /// The message that asks, and the thread it was written in.
    Asked(ChatMessage, String),
    AlreadyAsked,
}

/// The bot's answer, or the reason there is none. `ok` says which.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BotReply {
    pub ok: bool,
    pub text: String,
}

pub struct BotChat {
    bots_dir: PathBuf,
    claude: Arc<ClaudeCli>,
    file: Mutex<()>,
    /// The threads whose reply is being written right now, as `<bot>:<thread>`. A bot answers in several
    /// threads at once, each on its own.
    busy: Mutex<HashSet<String>>,
    /// What each thread of a bot must hear about its runs, in order, until it has time to look. The key
    /// is `<bot>:<label>`, the label being the thread, or the channel a run came from.
    wakes: Mutex<HashMap<String, VecDeque<String>>>,
    /// Shows a reply a bot wrote on its own, such as one answering a run's news.
    announce: OnceLock<Announce>,
    /// What each thread has said since its memory was last filed, keyed `<bot>:<thread>`.
    captures: Mutex<HashMap<String, Captured>>,
    /// Memory capture asks the CLI one thing at a time.
    capturing: Mutex<()>,
    /// Whether to file what a conversation says without the bot asking. Off until the app attaches it.
    capture_on: OnceLock<Arc<dyn Fn() -> bool + Send + Sync>>,
}

/// How many turns of a thread go by before what it said is filed into the bot's memory.
const CAPTURE_TURNS: u32 = 4;

#[derive(Default)]
struct Captured {
    turns: u32,
    /// How many messages of the thread were already read.
    upto: usize,
}

/// Marks a bot, or a channel, as answering for as long as it is held.
pub(super) struct Turn<'a> {
    pub(super) busy: &'a Mutex<HashSet<String>>,
    pub(super) id: String,
}

impl Drop for Turn<'_> {
    fn drop(&mut self) {
        lock(self.busy).remove(&self.id);
    }
}

impl BotChat {
    pub fn new(bots_dir: PathBuf, claude: Arc<ClaudeCli>) -> Self {
        Self {
            bots_dir,
            claude,
            file: Mutex::new(()),
            busy: Mutex::new(HashSet::new()),
            wakes: Mutex::new(HashMap::new()),
            announce: OnceLock::new(),
            captures: Mutex::new(HashMap::new()),
            capturing: Mutex::new(()),
            capture_on: OnceLock::new(),
        }
    }

    /// Connects the setting that turns memory capture on or off.
    pub fn attach_capture(&self, on: Arc<dyn Fn() -> bool + Send + Sync>) {
        let _ = self.capture_on.set(on);
    }

    /// Counts a finished turn of a thread and, every few turns, files what it said into the bot's
    /// memory on a thread of its own.
    pub fn after_turn(chat: Arc<BotChat>, bot: String, thread: String) {
        if chat.capture_on.get().is_some_and(|on| on()) && chat.count_turn(&bot, &thread) {
            std::thread::spawn(move || chat.file_memory(&bot, &thread));
        }
    }

    /// Counts one turn of a thread. True when it was the one that makes it time to file.
    fn count_turn(&self, bot: &str, thread: &str) -> bool {
        let mut captures = lock(&self.captures);
        let state = captures.entry(turn_key(bot, thread)).or_default();
        state.turns += 1;
        state.turns >= CAPTURE_TURNS
    }

    /// Files what a thread said since the last time, if it is worth asking the CLI about.
    fn file_memory(&self, bot: &str, thread: &str) {
        let key = turn_key(bot, thread);
        let upto = {
            let mut captures = lock(&self.captures);
            let Some(state) = captures.get_mut(&key) else {
                return;
            };
            state.turns = 0;
            state.upto
        };
        if let Some(read) = self.capture(bot, thread, upto)
            && let Some(state) = lock(&self.captures).get_mut(&key)
        {
            state.upto = read;
        }
    }

    /// Asks the CLI what is new in the thread's messages from `upto` on and files it. Returns how many
    /// messages were read, or None when the thread was answering or the CLI did not answer, so the
    /// same messages are read again next time.
    fn capture(&self, bot: &str, thread: &str, upto: usize) -> Option<usize> {
        if lock(&self.busy).contains(&turn_key(bot, thread)) {
            return None;
        }
        let folder = BotFolder::existing(&self.bots_dir, bot).ok()?;
        let dir = bot_thread::dir(folder.dir(), thread).ok()?;
        let history = {
            let _guard = self.lock();
            bot_conversation::read(&dir).ok()?
        };
        let fresh = history.get(upto.min(history.len())..)?;
        if !fresh.iter().any(|turn| turn.author == Author::User) {
            return Some(history.len());
        }
        let name = folder.settings().ok()?.name;
        let turns = bot_conversation::lines(fresh, &|turn| match turn.author {
            Author::User => "Pessoa".to_owned(),
            _ => name.clone(),
        });
        let _one = lock(&self.capturing);
        let question = bot_memory_capture::prompt(
            &bot_memory::read(folder.dir()),
            &bot_topics::list(folder.dir()),
            &turns,
        );
        let answer = self
            .claude
            .ask_in(folder.dir(), bot_memory_capture::INSTRUCTIONS, &question);
        if !answer.ok {
            return None;
        }
        let today = Local::now().format("%Y-%m-%d").to_string();
        let filed = bot_memory_capture::apply(
            folder.dir(),
            &bot_memory_capture::parse(&answer.text),
            &today,
        );
        log::info!("memory capture: {bot} filed {filed} note(s)");
        Some(history.len())
    }

    /// Connects the chat to the chat screen, which shows the replies a bot writes on its own.
    pub fn attach_announce(&self, announce: Announce) {
        let _ = self.announce.set(announce);
    }

    /// Queues a note for a bot, for the thread (or channel) the run came from: what changed in its
    /// runs. It is read there on the bot's next turn.
    pub fn wake(&self, bot: &str, label: &str, note: &str) {
        self.wakes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(wake_key(bot, label))
            .or_default()
            .push_back(note.to_owned());
    }

    /// Lets a bot read its queued notes for a thread now, on a thread of its own, unless that thread is
    /// answering. A note that finds it busy waits; the turn that ends the answer runs this again.
    pub fn kick(chat: Arc<BotChat>, bot: String, label: String) {
        std::thread::spawn(move || chat.run_wakes(&bot, &label));
    }

    /// Runs the queued notes for a thread, one turn each, and shows each reply it writes. A note from a
    /// channel's run is answered in the bot's most recent thread.
    fn run_wakes(&self, bot: &str, label: &str) {
        loop {
            let Some(note) = self.next_wake(bot, label) else {
                return;
            };
            let Ok(thread) = self.thread_for(bot, label) else {
                return;
            };
            match self.resume(bot, &thread, &note, &mut |_| {}) {
                Ok(message) => {
                    if let Some(announce) = self.announce.get() {
                        announce(&bot_thread::key_of(bot, &thread), &message);
                    }
                }
                Err(BotError::Busy) => {
                    self.requeue(bot, label, note);
                    return;
                }
                Err(_) => return,
            }
        }
    }

    fn next_wake(&self, bot: &str, label: &str) -> Option<String> {
        self.wakes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get_mut(&wake_key(bot, label))?
            .pop_front()
    }

    fn requeue(&self, bot: &str, label: &str, note: String) {
        self.wakes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(wake_key(bot, label))
            .or_default()
            .push_front(note);
    }

    /// The bot's threads, the most recently active first. A bot with none gets its first.
    pub fn threads(&self, id: &str) -> Result<Vec<ThreadSummary>, BotError> {
        bot_thread::list(BotFolder::existing(&self.bots_dir, id)?.dir())
    }

    /// Starts a new thread with a bot.
    pub fn create_thread(&self, id: &str) -> Result<ThreadSummary, BotError> {
        bot_thread::create(BotFolder::existing(&self.bots_dir, id)?.dir())
    }

    pub fn rename_thread(
        &self,
        id: &str,
        thread: &str,
        title: &str,
    ) -> Result<ThreadSummary, BotError> {
        bot_thread::rename(
            BotFolder::existing(&self.bots_dir, id)?.dir(),
            thread,
            title,
        )
    }

    /// Removes a thread. One that is answering cannot be removed, since the answer is written in it.
    pub fn delete_thread(&self, id: &str, thread: &str) -> Result<(), BotError> {
        let busy = lock(&self.busy);
        if busy.contains(&turn_key(id, thread)) {
            return Err(BotError::Busy);
        }
        bot_thread::delete(BotFolder::existing(&self.bots_dir, id)?.dir(), thread)
    }

    /// Stops the answer a bot is writing in a thread. Returns whether there was one.
    pub fn stop(&self, id: &str, thread: &str) -> bool {
        self.claude.stop(id, thread)
    }

    /// The thread a run belongs to: the one it is named for, or, for a run that came from a channel or
    /// from a thread that is gone, the bot's most recent thread.
    pub fn thread_for(&self, bot: &str, label: &str) -> Result<String, BotError> {
        let folder = BotFolder::existing(&self.bots_dir, bot)?;
        if bot_thread::dir(folder.dir(), label).is_ok() {
            return Ok(label.to_owned());
        }
        bot_thread::latest(folder.dir())
    }

    /// The folder of one of the bot's threads.
    fn thread_dir(&self, id: &str, thread: &str) -> Result<PathBuf, BotError> {
        bot_thread::dir(BotFolder::existing(&self.bots_dir, id)?.dir(), thread)
    }

    /// The conversation of a thread, oldest message first.
    pub fn history(&self, id: &str, thread: &str) -> Result<Vec<ChatMessage>, BotError> {
        let dir = self.thread_dir(id, thread)?;
        let _guard = self.lock();
        bot_conversation::read(&dir)
    }

    /// Sends the user's message in a thread and waits for the bot's answer. `on_text` gets each piece
    /// of the reply as the engine writes it. The message is kept even when the answer fails, and a
    /// failed answer is kept as an error line, so the conversation shows what happened.
    pub fn send_streaming(
        &self,
        id: &str,
        thread: &str,
        text: &str,
        on_text: &mut dyn FnMut(&str),
    ) -> Result<BotReply, BotError> {
        let text = text.trim();
        if text.is_empty() || text.chars().count() > MAX_MESSAGE {
            return Err(BotError::InvalidMessage);
        }
        let (folder, _turn) = self.begin(id, thread)?;
        let dir = bot_thread::dir(folder.dir(), thread)?;
        let settings = folder.settings()?;
        let recalled = bot_recall::recall(folder.dir(), thread, text);
        let prompt = {
            let _guard = self.lock();
            let history = bot_conversation::read(&dir)?;
            if history.is_empty() {
                bot_thread::title_from_first_message(folder.dir(), thread, text);
            }
            let note = turn_note::note(
                Local::now().fixed_offset(),
                &settings.name,
                settings.team.as_deref(),
                &[],
            );
            let recalled = recalled
                .map(|text| format!("{text}\n\n"))
                .unwrap_or_default();
            format!(
                "{note}\n\n{recalled}{}",
                bot_conversation::prompt(&history, &settings.name, text)
            )
        };

        let question = ChatMessage::new(Author::User, text);
        self.record(&dir, std::slice::from_ref(&question))?;
        let answer = self.claude.ask_streaming(id, thread, &prompt, on_text);
        let reply = reply_of(&answer);
        // Under the message it answers: a request the bot made during the turn was saved before it.
        {
            let _guard = self.lock();
            bot_conversation::insert_after(&dir, &question.id, &reply)?;
        }
        Ok(BotReply {
            ok: answer.ok,
            text: answer.text,
        })
    }

    /// Wakes a bot in a thread, after the person answered its request or something happened in its
    /// runs. The bot answers `note` with the thread's conversation so far. Only its answer is kept:
    /// `note` is Lynx's, not a message the person wrote.
    pub fn resume(
        &self,
        id: &str,
        thread: &str,
        note: &str,
        on_text: &mut dyn FnMut(&str),
    ) -> Result<ChatMessage, BotError> {
        let (folder, _turn) = self.begin(id, thread)?;
        let dir = bot_thread::dir(folder.dir(), thread)?;
        let settings = folder.settings()?;
        let prompt = {
            let _guard = self.lock();
            let history = bot_conversation::read(&dir)?;
            let lines = bot_conversation::lines(&history, &|turn| match turn.author {
                Author::User => "Pessoa".to_owned(),
                _ => settings.name.clone(),
            });
            let now = turn_note::note(
                Local::now().fixed_offset(),
                &settings.name,
                settings.team.as_deref(),
                &[],
            );
            format!(
                "{now}\n\nConversa até agora, da mais antiga para a mais nova:\n{lines}\n\n[Lynx] {note}"
            )
        };
        let answer = self.claude.ask_streaming(id, thread, &prompt, on_text);
        let reply = reply_of(&answer);
        self.record(&dir, std::slice::from_ref(&reply))?;
        Ok(reply)
    }

    /// Adds a note to the bot's memory, dated today.
    pub fn memory_add(&self, id: &str, entry: &str) -> Result<bot_memory::Added, BotError> {
        let folder = BotFolder::existing(&self.bots_dir, id)?;
        let today = Local::now().format("%Y-%m-%d").to_string();
        bot_memory::add(folder.dir(), entry, &today)
    }

    /// Removes the notes of the bot's memory that mention `snippet`.
    pub fn memory_remove(&self, id: &str, snippet: &str) -> Result<usize, BotError> {
        let folder = BotFolder::existing(&self.bots_dir, id)?;
        bot_memory::remove(folder.dir(), snippet)
    }

    /// A topic of the bot's memory.
    pub fn topic_read(&self, id: &str, topic: &str) -> Result<String, BotError> {
        let folder = BotFolder::existing(&self.bots_dir, id)?;
        bot_topics::read(folder.dir(), topic)
    }

    /// Adds a note to a topic of the bot's memory, dated today, creating the topic when it is new.
    pub fn topic_add(
        &self,
        id: &str,
        topic: &str,
        title: Option<&str>,
        aliases: Option<&str>,
        entry: &str,
    ) -> Result<(), BotError> {
        let folder = BotFolder::existing(&self.bots_dir, id)?;
        let today = Local::now().format("%Y-%m-%d").to_string();
        bot_topics::append(folder.dir(), topic, title, aliases, entry, &today)
    }

    /// Replaces a topic of the bot's memory.
    pub fn topic_write(&self, id: &str, topic: &str, text: &str) -> Result<(), BotError> {
        let folder = BotFolder::existing(&self.bots_dir, id)?;
        bot_topics::write(folder.dir(), topic, text)
    }

    /// The command patterns a bot may run.
    pub fn commands(&self, id: &str) -> Result<Vec<String>, BotError> {
        command_permissions::allowed(&self.bots_dir, id)
    }

    /// The repositories where a bot may start agents.
    pub fn worker_grants(&self, id: &str) -> Result<Vec<Grant>, BotError> {
        bot_folder::worker_grants(&self.bots_dir, id)
    }

    /// The repositories a bot may read.
    pub fn grants(&self, id: &str) -> Result<Vec<Grant>, BotError> {
        bot_folder::grants(&self.bots_dir, id)
    }

    /// Asks the person for a permission, in the thread the run belongs to (`label`). A request still
    /// waiting is not asked again. The message that asks is kept in the conversation, so the chat shows it with
    /// its buttons.
    pub fn request_access(
        &self,
        id: &str,
        label: &str,
        request: AccessRequest,
        reason: &str,
    ) -> Result<RequestOutcome, BotError> {
        let thread = self.thread_for(id, label)?;
        let dir = self.thread_dir(id, &thread)?;
        let _guard = self.lock();
        let history = bot_conversation::read(&dir)?;
        let waiting = history.iter().any(|turn| {
            turn.request.as_ref().is_some_and(|asked| {
                asked.state == AccessState::Pending && asked.same_ask(&request)
            })
        });
        if waiting {
            return Ok(RequestOutcome::AlreadyAsked);
        }
        let message = ChatMessage::new(Author::Bot, reason)
            .spoken_by(id)
            .requesting(request);
        bot_conversation::append(&dir, std::slice::from_ref(&message))?;
        Ok(RequestOutcome::Asked(message, thread))
    }

    /// The request a message of a thread asks, while it still waits for the person's answer.
    pub fn pending_request(
        &self,
        id: &str,
        thread: &str,
        message_id: &str,
    ) -> Result<AccessRequest, BotError> {
        let dir = self.thread_dir(id, thread)?;
        let _guard = self.lock();
        bot_conversation::read(&dir)?
            .into_iter()
            .find(|turn| turn.id == message_id)
            .and_then(|turn| turn.request)
            .filter(|asked| asked.state == AccessState::Pending)
            .ok_or(BotError::UnknownRequest)
    }

    /// Answers a request: approved or denied. Only a request still waiting can be answered, and once.
    pub fn decide_request(
        &self,
        id: &str,
        thread: &str,
        message_id: &str,
        state: AccessState,
    ) -> Result<AccessRequest, BotError> {
        let dir = self.thread_dir(id, thread)?;
        let _guard = self.lock();
        let mut history = bot_conversation::read(&dir)?;
        let asked = history
            .iter_mut()
            .find(|turn| turn.id == message_id)
            .and_then(|turn| turn.request.as_mut())
            .filter(|asked| asked.state == AccessState::Pending)
            .ok_or(BotError::UnknownRequest)?;
        asked.state = state;
        let decided = asked.clone();
        bot_conversation::write(&dir, &history)?;
        Ok(decided)
    }

    /// Removes a bot and its conversation. A bot that is answering, in any thread, cannot be removed until
    /// it has finished.
    pub fn delete(&self, id: &str) -> Result<(), BotError> {
        let busy = lock(&self.busy);
        let prefix = format!("{id}:");
        if busy.iter().any(|key| key.starts_with(&prefix)) {
            return Err(BotError::Busy);
        }
        bot_folder::delete(&self.bots_dir, id)
    }

    /// Checks the bot exists and marks one of its threads answering, so a delete or a second answer in
    /// the same thread waits for this one.
    fn begin(&self, id: &str, thread: &str) -> Result<(BotFolder, Turn<'_>), BotError> {
        let mut busy = lock(&self.busy);
        let folder = BotFolder::existing(&self.bots_dir, id)?;
        if !busy.insert(turn_key(id, thread)) {
            return Err(BotError::Busy);
        }
        Ok((
            folder,
            Turn {
                busy: &self.busy,
                id: turn_key(id, thread),
            },
        ))
    }

    fn record(&self, dir: &Path, messages: &[ChatMessage]) -> Result<(), BotError> {
        let _guard = self.lock();
        bot_conversation::append(dir, messages)
    }

    fn lock(&self) -> MutexGuard<'_, ()> {
        self.file.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A reply, as it is kept: what the bot wrote, what it wrote before the person stopped it, or the
/// reason there is none.
pub(super) fn reply_of(answer: &ClaudeAnswer) -> ChatMessage {
    if answer.stopped {
        ChatMessage::new(Author::Bot, &answer.text).stopped()
    } else if answer.ok {
        ChatMessage::new(Author::Bot, &answer.text)
    } else {
        ChatMessage::new(Author::Error, &answer.text)
    }
}

/// What marks a thread as answering.
fn turn_key(bot: &str, thread: &str) -> String {
    format!("{bot}:{thread}")
}

/// What keys the notes waiting for a thread.
fn wake_key(bot: &str, label: &str) -> String {
    format!("{bot}:{label}")
}

fn lock<T>(busy: &Mutex<T>) -> MutexGuard<'_, T> {
    busy.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn chat(dir: &TempDir) -> BotChat {
        let bots = dir.path().join("bots");
        let claude = Arc::new(ClaudeCli::new(bots.clone(), dir.path().to_path_buf()));
        BotChat::new(bots, claude)
    }

    #[test]
    fn an_unknown_bot_has_no_history_and_no_folder_is_made() {
        let dir = TempDir::new().expect("a temporary folder");
        let chat = chat(&dir);
        assert!(matches!(
            chat.history("ninguem", "x"),
            Err(BotError::UnknownBot)
        ));
        assert!(!dir.path().join("bots").join("ninguem").exists());
    }

    #[test]
    fn a_message_to_an_unknown_bot_is_refused_and_leaves_nothing_busy() {
        let dir = TempDir::new().expect("a temporary folder");
        let chat = chat(&dir);
        assert!(matches!(
            chat.send_streaming("ninguem", "x", "oi", &mut |_| {}),
            Err(BotError::UnknownBot)
        ));
        assert!(lock(&chat.busy).is_empty());
    }

    #[test]
    fn an_empty_or_long_message_is_refused_before_anything_is_read() {
        let dir = TempDir::new().expect("a temporary folder");
        let chat = chat(&dir);
        assert!(matches!(
            chat.send_streaming("ada", "x", "   ", &mut |_| {}),
            Err(BotError::InvalidMessage)
        ));
        let long = "a".repeat(MAX_MESSAGE + 1);
        assert!(matches!(
            chat.send_streaming("ada", "x", &long, &mut |_| {}),
            Err(BotError::InvalidMessage)
        ));
    }

    #[test]
    fn a_bot_has_a_thread_from_the_start_and_each_thread_keeps_its_own_conversation() {
        let dir = TempDir::new().expect("a temporary folder");
        BotFolder::open(&dir.path().join("bots"), "ada").expect("the folder opens");
        let chat = chat(&dir);

        let first = chat.threads("ada").expect("threads");
        assert_eq!(first.len(), 1, "a chat always has somewhere to write");
        let second = chat.create_thread("ada").expect("a second thread");
        let first_dir = chat.thread_dir("ada", &first[0].id).expect("dir");
        chat.record(&first_dir, &[ChatMessage::new(Author::User, "só aqui")])
            .expect("recorded");

        assert_eq!(chat.history("ada", &first[0].id).expect("history").len(), 1);
        assert!(chat.history("ada", &second.id).expect("history").is_empty());
        assert!(matches!(
            chat.history("ada", "../../etc"),
            Err(BotError::UnknownThread)
        ));
    }

    #[test]
    fn only_the_thread_that_answers_is_kept_from_deletion_and_a_run_finds_its_thread() {
        let dir = TempDir::new().expect("a temporary folder");
        BotFolder::open(&dir.path().join("bots"), "ada").expect("the folder opens");
        let chat = chat(&dir);
        let one = chat.threads("ada").expect("threads")[0].id.clone();
        let two = chat.create_thread("ada").expect("a second thread").id;

        lock(&chat.busy).insert(turn_key("ada", &two));
        assert!(matches!(
            chat.delete_thread("ada", &two),
            Err(BotError::Busy)
        ));
        chat.delete_thread("ada", &one)
            .expect("the other thread is free to go");
        lock(&chat.busy).remove(&turn_key("ada", &two));

        assert_eq!(chat.thread_for("ada", &two).expect("its own"), two);
        assert_eq!(
            chat.thread_for("ada", "ch-dev").expect("a channel's run"),
            two,
            "a run that came from a channel uses the most recent thread"
        );
        assert_eq!(
            chat.thread_for("ada", &one).expect("a thread that is gone"),
            two
        );
    }

    #[test]
    fn two_threads_of_a_bot_can_answer_at_once_and_each_is_marked() {
        let dir = TempDir::new().expect("a temporary folder");
        BotFolder::open(&dir.path().join("bots"), "ada").expect("the folder opens");
        let chat = chat(&dir);
        let one = chat.threads("ada").expect("threads")[0].id.clone();
        let two = chat.create_thread("ada").expect("a second thread").id;

        let (_, first) = chat.begin("ada", &one).expect("the first thread starts");
        let second = chat.begin("ada", &two);
        assert!(
            second.is_ok(),
            "another thread of the same bot is not kept waiting"
        );
        assert!(
            matches!(chat.begin("ada", &one), Err(BotError::Busy)),
            "the same thread is"
        );
        assert!(
            matches!(chat.delete("ada"), Err(BotError::Busy)),
            "the bot cannot go while any answers"
        );

        drop(first);
        drop(second);
        assert!(lock(&chat.busy).is_empty());
    }

    #[test]
    fn there_is_nothing_to_stop_when_the_bot_is_not_answering() {
        let dir = TempDir::new().expect("a temporary folder");
        assert!(!chat(&dir).stop("ada", "t1"));
    }

    #[test]
    fn a_bot_that_is_answering_cannot_be_deleted_until_it_finishes() {
        let dir = TempDir::new().expect("a temporary folder");
        let bots = dir.path().join("bots");
        BotFolder::open(&bots, "ada").expect("the folder opens");
        let chat = chat(&dir);
        lock(&chat.busy).insert(turn_key("ada", "t1"));

        assert!(matches!(chat.delete("ada"), Err(BotError::Busy)));
        assert!(bots.join("ada").exists());

        lock(&chat.busy).remove(&turn_key("ada", "t1"));
        chat.delete("ada")
            .expect("deleted once it is not answering");
        assert!(!bots.join("ada").exists());
    }

    #[test]
    fn every_fourth_turn_of_a_thread_is_the_time_to_file_its_memory() {
        let dir = TempDir::new().expect("a temporary folder");
        let chat = chat(&dir);

        let due: Vec<bool> = (0..4).map(|_| chat.count_turn("ada", "t1")).collect();
        assert_eq!(due, [false, false, false, true]);
        assert!(!chat.count_turn("ada", "t2"), "another thread counts apart");

        // Filing starts the count again.
        chat.file_memory("ada", "t1");
        assert_eq!(
            (0..4)
                .map(|_| chat.count_turn("ada", "t1"))
                .collect::<Vec<_>>(),
            [false, false, false, true]
        );
    }

    /// Talks to a real bot for four turns, waits for its memory to be filed, then asks in a new thread
    /// for something only a topic holds. It needs the Claude CLI signed in, so it is ignored by default:
    /// `cargo test bot_remembers_across -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn bot_remembers_across_threads_through_captured_topics() {
        use super::super::bot_folder::{self, BotSetup};

        let root = TempDir::new().expect("a temporary folder");
        let bots = root.path().join("bots");
        let folder = BotFolder::open(&bots, "ada").expect("the bot is made");
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
        let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
        let chat = Arc::new(BotChat::new(
            bots.clone(),
            Arc::new(ClaudeCli::new(bots, home)),
        ));
        chat.attach_capture(Arc::new(|| true));

        let first = chat.threads("ada").expect("threads").remove(0).id;
        for message in [
            "Oi! Eu me chamo Marina e sou vegetariana.",
            "Vou para Lisboa dia 12 de dezembro e o meu hotel fica no bairro do Chiado.",
            "Quero um roteiro de comida vegetariana por lá.",
            "Obrigada! Responda só com ok.",
        ] {
            chat.send_streaming("ada", &first, message, &mut |_| {})
                .expect("answered");
            BotChat::after_turn(Arc::clone(&chat), "ada".to_owned(), first.clone());
        }
        let started = std::time::Instant::now();
        while bot_topics::list(folder.dir()).is_empty() && started.elapsed().as_secs() < 90 {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        println!("MEMORY:\n{}", bot_memory::read(folder.dir()));
        println!("TOPICS: {:?}", bot_topics::list(folder.dir()));
        assert!(
            !bot_topics::list(folder.dir()).is_empty(),
            "nothing was filed"
        );

        let second = chat.create_thread("ada").expect("thread").id;
        let reply = chat
            .send_streaming(
                "ada",
                &second,
                "Em que bairro fica o meu hotel mesmo?",
                &mut |_| {},
            )
            .expect("answered");
        println!("REPLY: {}", reply.text);
        assert!(reply.text.contains("Chiado"), "{}", reply.text);
    }
}
