//! A channel's conversation. Who answers a message is decided in order: the members named with @;
//! the only member there is; Jev, when the user turned it on; and otherwise the router, which reads
//! the conversation and picks. The members answer one after the other, each reading the conversation
//! up to that point, so a later one can react to an earlier one.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

use chrono::{DateTime, FixedOffset, Local};
use serde_json::Value;

use super::bot_chat::{Turn, reply_of};
use super::bot_conversation::{self, Author, ChatMessage, MAX_MESSAGE};
use super::bot_folder::{self, BotDetail, BotError};
use super::channel::{self, ChannelSettings, ChannelSummary, NewChannel};
use super::claude_cli::ClaudeCli;
use super::jev_decider::{Member, Question, RoomDecider, Route};
use super::turn_note;

/// Told to the router: it only picks who answers, and never answers the person.
const ROUTER_INSTRUCTIONS: &str = "Você escolhe quais membros de um canal respondem à última mensagem da pessoa. Responda somente com JSON, no formato {\"answer\": [\"id\", ...]}, com os ids dos membros que devem responder, na ordem em que devem falar. Use uma lista vazia se ninguém deve responder. Não escreva mais nada.";
/// How much of a member's instructions the router reads, in characters.
const MEMBER_SUMMARY: usize = 160;

/// What a channel's send reports while it runs. The app turns each call into an event.
pub trait ChannelSink {
    /// A piece of an answer, as the member writes it.
    fn text(&mut self, author: &str, piece: &str);
    /// Where the answer stands: `routing`, `answering` with the member writing, or `idle`. `waiting`
    /// are the members who answer after that one.
    fn status(&mut self, phase: &str, author: Option<&str>, waiting: &[String]);
    /// A message saved to the conversation.
    fn message(&mut self, message: &ChatMessage);
}

pub struct ChannelChat {
    bots_dir: PathBuf,
    channels_dir: PathBuf,
    claude: Arc<ClaudeCli>,
    file: Mutex<()>,
    /// The channels whose members are answering right now.
    busy: Mutex<HashSet<String>>,
    /// Jev, once the app has one. Without it the router picks.
    decider: OnceLock<Arc<dyn RoomDecider>>,
    /// What stops the answer of each channel that is answering.
    stops: Mutex<HashMap<String, Arc<StopState>>>,
}

/// Whether the person stopped a channel's answer, and who is writing, so that one is stopped too.
#[derive(Default)]
struct StopState {
    stopped: AtomicBool,
    speaker: Mutex<Option<String>>,
}

impl ChannelChat {
    pub fn new(bots_dir: PathBuf, channels_dir: PathBuf, claude: Arc<ClaudeCli>) -> Self {
        Self {
            bots_dir,
            channels_dir,
            claude,
            file: Mutex::new(()),
            busy: Mutex::new(HashSet::new()),
            decider: OnceLock::new(),
            stops: Mutex::new(HashMap::new()),
        }
    }

    /// Stops the answer of a channel: the member writing now, and the ones waiting to write. What was
    /// written stays. Returns whether the channel was answering.
    pub fn stop(&self, id: &str) -> bool {
        let state = self
            .stops
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(id)
            .cloned();
        let Some(state) = state else {
            return false;
        };
        state.stopped.store(true, Ordering::SeqCst);
        let speaker = state
            .speaker
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if let Some(speaker) = speaker {
            self.claude.stop(&speaker, &run_label(id));
        }
        true
    }

    /// Lets a decision model pick who answers a message that names nobody.
    pub fn attach_decider(&self, decider: Arc<dyn RoomDecider>) {
        let _ = self.decider.set(decider);
    }

    pub fn list(&self) -> Result<Vec<ChannelSummary>, BotError> {
        channel::list(&self.channels_dir)
    }

    pub fn update_members(
        &self,
        id: &str,
        members: Vec<String>,
    ) -> Result<ChannelSummary, BotError> {
        channel::update_members(&self.channels_dir, &self.bots_dir, id, members)
    }

    pub fn create(&self, channel: NewChannel) -> Result<ChannelSummary, BotError> {
        channel::create(&self.channels_dir, &self.bots_dir, channel)
    }

    /// Removes a channel with its conversation. A channel that is answering cannot be removed until it
    /// has finished. The lock is held through the removal, so no answer can start in between.
    pub fn delete(&self, id: &str) -> Result<(), BotError> {
        let busy = self.busy.lock().unwrap_or_else(PoisonError::into_inner);
        if busy.contains(id) {
            return Err(BotError::ChannelBusy);
        }
        channel::delete(&self.channels_dir, id)
    }

    /// Checks the channel exists and marks it answering, so a delete or a second answer waits for this one.
    fn begin(&self, id: &str) -> Result<(PathBuf, ChannelSettings, Turn<'_>), BotError> {
        let mut busy = self.busy.lock().unwrap_or_else(PoisonError::into_inner);
        let (dir, settings) = channel::open(&self.channels_dir, id)?;
        if !busy.insert(id.to_owned()) {
            return Err(BotError::ChannelBusy);
        }
        let turn = Turn {
            busy: &self.busy,
            id: id.to_owned(),
        };
        Ok((dir, settings, turn))
    }

    /// The conversation of a channel, oldest message first.
    pub fn history(&self, id: &str) -> Result<Vec<ChatMessage>, BotError> {
        let (dir, _) = channel::open(&self.channels_dir, id)?;
        let _guard = self.lock();
        bot_conversation::read(&dir)
    }

    /// Sends the person's message and lets the members answer, one after the other. The sink hears each
    /// message as it is saved, the status of the answer, and the pieces of each answer as they are written.
    pub fn send_streaming(
        &self,
        id: &str,
        text: &str,
        sink: &mut dyn ChannelSink,
    ) -> Result<(), BotError> {
        let text = text.trim();
        if text.is_empty() || text.chars().count() > MAX_MESSAGE {
            return Err(BotError::InvalidMessage);
        }
        let (dir, settings, _turn) = self.begin(id)?;
        let members = self.members(&settings.members);
        let mut context = self.history_of(&dir)?;
        let message = ChatMessage::new(Author::User, text);
        self.save(&dir, message.clone(), sink)?;
        context.push(message);

        sink.status("routing", None, &[]);
        let order = self.order(&dir, &settings.name, text, &context, &members, sink)?;
        let mut queue: VecDeque<String> = order.into();
        let stop = Arc::new(StopState::default());
        self.stops
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(id.to_owned(), Arc::clone(&stop));
        let _registered = StopRegistered {
            stops: &self.stops,
            channel: id,
        };
        while let Some(member) = queue.pop_front() {
            if stop.stopped.load(Ordering::SeqCst) {
                break;
            }
            let Some(bot) = members.iter().find(|bot| bot.id == member) else {
                continue;
            };
            let waiting: Vec<String> = queue.iter().cloned().collect();
            sink.status("answering", Some(bot.id.as_str()), &waiting);
            let prompt = answer_prompt(Local::now().fixed_offset(), &context, &members, bot);
            *stop.speaker.lock().unwrap_or_else(PoisonError::into_inner) = Some(bot.id.clone());
            let answer =
                self.claude
                    .ask_streaming(&bot.id, &run_label(id), &prompt, &mut |piece| {
                        sink.text(&bot.id, piece)
                    });
            *stop.speaker.lock().unwrap_or_else(PoisonError::into_inner) = None;
            let reply = reply_of(&answer).spoken_by(&bot.id);
            if answer.ok && !answer.stopped {
                let handed = handoffs(&answer.text, &bot.id, &members, &queue);
                queue.extend(handed);
            }
            self.save(&dir, reply.clone(), sink)?;
            context.push(reply);
            if answer.stopped {
                break;
            }
        }
        sink.status("idle", None, &[]);
        Ok(())
    }

    /// Who answers, in order. Members named with @ answer and nobody else, and a channel with one
    /// member needs no choosing. Otherwise Jev picks one member, or all of them, and falls back to the
    /// first member when it is not sure. With Jev off, the router picks; a router that fails is kept
    /// as an error line, and then nobody answers.
    fn order(
        &self,
        dir: &Path,
        room: &str,
        text: &str,
        context: &[ChatMessage],
        members: &[BotDetail],
        sink: &mut dyn ChannelSink,
    ) -> Result<Vec<String>, BotError> {
        let named = mentioned(text, members);
        if !named.is_empty() {
            return Ok(named);
        }
        if let [only] = members {
            return Ok(vec![only.id.clone()]);
        }
        let Some(first) = members.first() else {
            return Ok(Vec::new());
        };
        if let Some(decider) = self.decider.get() {
            match decider.choose(&question_for(room, text, context, members)) {
                Route::Member { id, .. } => return Ok(vec![id]),
                Route::Everyone { .. } => {
                    return Ok(members.iter().map(|bot| bot.id.clone()).collect());
                }
                Route::Fallback(_) => return Ok(vec![first.id.clone()]),
                Route::Off => {}
            }
        }
        let routed = self
            .claude
            .ask_in(dir, ROUTER_INSTRUCTIONS, &route_prompt(context, members));
        if !routed.ok {
            self.save(dir, ChatMessage::new(Author::Error, &routed.text), sink)?;
            Ok(Vec::new())
        } else if let Some(picked) = parse_route(&routed.text, members) {
            Ok(picked)
        } else {
            let failure = ChatMessage::new(Author::Error, "Could not decide who answers.");
            self.save(dir, failure, sink)?;
            Ok(Vec::new())
        }
    }

    /// The members the channel names that are still bots. A bot deleted since is left out.
    fn members(&self, ids: &[String]) -> Vec<BotDetail> {
        ids.iter()
            .filter_map(|id| bot_folder::detail(&self.bots_dir, id).ok())
            .collect()
    }

    fn history_of(&self, dir: &Path) -> Result<Vec<ChatMessage>, BotError> {
        let _guard = self.lock();
        bot_conversation::read(dir)
    }

    fn record(&self, dir: &Path, messages: &[ChatMessage]) -> Result<(), BotError> {
        let _guard = self.lock();
        bot_conversation::append(dir, messages)
    }

    /// Saves a message, and tells the sink it is there.
    fn save(
        &self,
        dir: &Path,
        message: ChatMessage,
        sink: &mut dyn ChannelSink,
    ) -> Result<(), BotError> {
        self.record(dir, std::slice::from_ref(&message))?;
        sink.message(&message);
        Ok(())
    }

    fn lock(&self) -> MutexGuard<'_, ()> {
        self.file.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Takes a channel off the list of those that can be stopped when its answer ends.
struct StopRegistered<'a> {
    stops: &'a Mutex<HashMap<String, Arc<StopState>>>,
    channel: &'a str,
}

impl Drop for StopRegistered<'_> {
    fn drop(&mut self) {
        self.stops
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(self.channel);
    }
}

/// What the runs a channel starts are called, to tell them from the runs of a bot's threads.
fn run_label(channel: &str) -> String {
    format!("ch-{channel}")
}

/// Labels each line of a channel's conversation: the person, or the member who wrote it.
fn labeller(members: &[BotDetail]) -> impl Fn(&ChatMessage) -> String + '_ {
    move |turn| match turn.author {
        Author::User => "Pessoa".to_owned(),
        _ => turn
            .speaker
            .as_deref()
            .and_then(|id| members.iter().find(|bot| bot.id == id))
            .map_or_else(|| "Bot".to_owned(), |bot| bot.name.clone()),
    }
}

/// What Jev is asked: the room, the members with what each does, the lines before the new message, and
/// the new message itself.
fn question_for(
    room: &str,
    text: &str,
    context: &[ChatMessage],
    members: &[BotDetail],
) -> Question {
    let label = labeller(members);
    let earlier = context.len().saturating_sub(1);
    Question {
        room: room.to_owned(),
        humans: vec!["Pessoa".to_owned()],
        members: members
            .iter()
            .map(|bot| Member {
                id: bot.id.clone(),
                name: bot.name.clone(),
                title: bot.team.clone(),
                description: bot.instructions.clone(),
            })
            .collect(),
        recent: context[..earlier]
            .iter()
            .filter(|turn| turn.author != Author::Error)
            .map(|turn| (label(turn), turn.text.clone()))
            .collect(),
        message: ("Pessoa".to_owned(), text.to_owned()),
    }
}

fn route_prompt(context: &[ChatMessage], members: &[BotDetail]) -> String {
    let roster = members
        .iter()
        .map(|bot| {
            let team = bot
                .team
                .as_deref()
                .map(|team| format!(" ({team})"))
                .unwrap_or_default();
            let summary: String = bot
                .instructions
                .trim()
                .chars()
                .take(MEMBER_SUMMARY)
                .collect();
            format!("- {}: {}{team} — {summary}", bot.id, bot.name)
        })
        .collect::<Vec<_>>()
        .join("\n");
    let conversation = bot_conversation::lines(context, &labeller(members));
    format!(
        "Membros do canal:\n{roster}\n\nConversa até agora, da mais antiga para a mais nova:\n{conversation}\n\nDecida quem responde à última mensagem da pessoa."
    )
}

/// What a member is asked to answer: the note of the turn, the conversation, and the message to answer.
fn answer_prompt(
    now: DateTime<FixedOffset>,
    context: &[ChatMessage],
    members: &[BotDetail],
    bot: &BotDetail,
) -> String {
    let conversation = bot_conversation::lines(context, &labeller(members));
    let roster: Vec<String> = members.iter().map(member_label).collect();
    let note = turn_note::note(now, &bot.name, bot.team.as_deref(), &roster);
    format!(
        "{note}\n\nConversa neste canal, da mais antiga para a mais nova:\n{conversation}\n\nResponda à última mensagem, como {}.",
        bot.name
    )
}

/// A member as the channel names it: its name, and its team in brackets when it has one.
fn member_label(bot: &BotDetail) -> String {
    match &bot.team {
        Some(team) => format!("{} ({team})", bot.name),
        None => bot.name.clone(),
    }
}

/// The ids the router picked, from the JSON it wrote somewhere in its answer. Ids that are not
/// members are dropped, and a repeated one counts once. No JSON at all is `None`.
fn parse_route(text: &str, members: &[BotDetail]) -> Option<Vec<String>> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end < start {
        return None;
    }
    let value: Value = serde_json::from_str(&text[start..=end]).ok()?;
    let mut picked: Vec<String> = Vec::new();
    for id in value
        .get("answer")?
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
    {
        let is_member = members.iter().any(|bot| bot.id == id);
        if is_member && !picked.iter().any(|picked| picked == id) {
            picked.push(id.to_owned());
        }
    }
    Some(picked)
}

/// The members a message names with @, by name, in the order of the channel.
fn mentioned(text: &str, members: &[BotDetail]) -> Vec<String> {
    let lower = text.to_lowercase();
    members
        .iter()
        .filter(|bot| lower.contains(&format!("@{}", bot.name.to_lowercase())))
        .map(|bot| bot.id.clone())
        .collect()
}

/// The members a reply hands the turn to: the ones it names with @, except the speaker itself and the
/// ones already waiting, which will answer anyway.
fn handoffs(
    reply: &str,
    speaker: &str,
    members: &[BotDetail],
    waiting: &VecDeque<String>,
) -> Vec<String> {
    mentioned(reply, members)
        .into_iter()
        .filter(|id| id.as_str() != speaker && !waiting.contains(id))
        .collect()
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use tempfile::TempDir;

    use super::*;

    /// A decider that always gives one route, and keeps the new message of each question it was asked.
    struct Fixed(Route, Mutex<Vec<(String, usize)>>);

    impl Fixed {
        fn new(route: Route) -> Arc<Self> {
            Arc::new(Self(route, Mutex::new(Vec::new())))
        }

        fn asked(&self) -> Vec<(String, usize)> {
            self.1.lock().expect("the log").clone()
        }
    }

    impl RoomDecider for Fixed {
        fn choose(&self, question: &Question) -> Route {
            self.1
                .lock()
                .expect("the log")
                .push((question.message.1.clone(), question.recent.len()));
            self.0.clone()
        }
    }

    fn chat_in(root: &Path) -> ChannelChat {
        let bots = root.join("bots");
        let claude = Arc::new(ClaudeCli::new(bots.clone(), root.to_path_buf()));
        ChannelChat::new(bots, root.join("channels"), claude)
    }

    fn who_answers(
        chat: &ChannelChat,
        root: &Path,
        text: &str,
        context: &[ChatMessage],
    ) -> Vec<String> {
        let members = [member("ada", "Ada"), member("bento", "Bento")];
        chat.order(root, "dev", text, context, &members, &mut Silent)
            .expect("an order")
    }

    #[test]
    fn a_member_named_with_at_answers_alone_and_jev_is_not_asked() {
        let root = TempDir::new().expect("temp dir");
        let chat = chat_in(root.path());
        let jev = Fixed::new(Route::Everyone { probability: 0.9 });
        chat.attach_decider(jev.clone());

        assert_eq!(
            who_answers(&chat, root.path(), "@Bento olha isso", &[]),
            ["bento"]
        );
        assert!(jev.asked().is_empty());
    }

    #[test]
    fn a_channel_with_one_member_needs_no_choosing() {
        let root = TempDir::new().expect("temp dir");
        let chat = chat_in(root.path());
        let jev = Fixed::new(Route::Fallback("not sure"));
        chat.attach_decider(jev.clone());

        let order = chat
            .order(
                root.path(),
                "dev",
                "oi",
                &[],
                &[member("ada", "Ada")],
                &mut Silent,
            )
            .expect("an order");

        assert_eq!(order, ["ada"]);
        assert!(jev.asked().is_empty());
    }

    #[test]
    fn jev_picks_one_member_or_all_and_falls_back_to_the_first() {
        let root = TempDir::new().expect("temp dir");
        let earlier = ChatMessage::new(Author::User, "bom dia");
        let new = ChatMessage::new(Author::User, "quem revisa o botão?");
        let context = [earlier, new];

        let chat = chat_in(root.path());
        let jev = Fixed::new(Route::Member {
            id: "bento".to_owned(),
            probability: 0.9,
        });
        chat.attach_decider(jev.clone());
        assert_eq!(
            who_answers(&chat, root.path(), "quem revisa o botão?", &context),
            ["bento"]
        );
        assert_eq!(
            jev.asked(),
            [("quem revisa o botão?".to_owned(), 1)],
            "the new message is the question, and only the lines before it are context"
        );

        let chat = chat_in(root.path());
        chat.attach_decider(Fixed::new(Route::Everyone { probability: 0.8 }));
        assert_eq!(
            who_answers(&chat, root.path(), "todos", &context),
            ["ada", "bento"]
        );

        let chat = chat_in(root.path());
        chat.attach_decider(Fixed::new(Route::Fallback("timed out")));
        assert_eq!(who_answers(&chat, root.path(), "hm", &context), ["ada"]);
    }

    fn member(id: &str, name: &str) -> BotDetail {
        BotDetail {
            id: id.to_owned(),
            name: name.to_owned(),
            color: "sky".to_owned(),
            provider: "claude".to_owned(),
            model: None,
            instructions: "Ajude.".to_owned(),
            folder: String::new(),
            team: None,
        }
    }

    #[test]
    fn a_member_is_told_the_time_its_name_and_who_else_is_in_the_channel() {
        let ada = BotDetail {
            team: Some("infra".to_owned()),
            ..member("ada", "Ada")
        };
        let members = vec![ada, member("bento", "Bento")];
        let now = FixedOffset::west_opt(3 * 3600)
            .expect("a valid offset")
            .with_ymd_and_hms(2026, 10, 23, 14, 5, 0)
            .single()
            .expect("a real time");

        let prompt = answer_prompt(now, &[], &members, &members[0]);

        assert!(prompt.starts_with(
            "Agora: sexta-feira, 23/10/2026, 14:05 (UTC-03:00).\nVocê é Ada (equipe infra).\nMembros do canal: Ada (infra), Bento.\n\n"
        ));
        assert!(prompt.ends_with("como Ada."));
    }

    #[test]
    fn a_reply_hands_the_turn_to_the_members_it_names() {
        let members = [
            member("ada", "Ada"),
            member("bento", "Bento"),
            member("carla", "Carla"),
        ];
        let waiting: VecDeque<String> = ["carla".to_owned()].into();

        assert_eq!(
            handoffs("Pronto. @Bento, revise.", "ada", &members, &waiting),
            vec!["bento".to_owned()]
        );
        assert!(
            handoffs(
                "Feito, não preciso de mais ninguém.",
                "ada",
                &members,
                &waiting
            )
            .is_empty()
        );
        // A member already waiting is not queued twice, and a member does not hand to itself.
        assert!(handoffs("@Carla e @Ada, sigam.", "ada", &members, &waiting).is_empty());
    }

    #[test]
    fn the_router_picks_members_once_and_only_members() {
        let members = [member("ada", "Ada"), member("bento", "Bento")];

        let picked = parse_route(
            r#"{"answer": ["bento", "ghost", "bento", "ada"]}"#,
            &members,
        );
        assert_eq!(picked, Some(vec!["bento".to_owned(), "ada".to_owned()]));
        assert_eq!(
            parse_route("```json\n{\"answer\": []}\n```", &members),
            Some(Vec::new())
        );
        assert_eq!(parse_route("não sei dizer", &members), None);
    }

    #[test]
    fn a_name_written_with_at_is_a_mention() {
        let members = [member("ada", "Ada"), member("bento", "Bento")];

        assert_eq!(
            mentioned("@bento, você viu isso?", &members),
            vec!["bento".to_owned()]
        );
        assert!(mentioned("sem menção", &members).is_empty());
    }

    #[test]
    fn a_channel_that_does_not_exist_or_an_empty_message_is_refused() {
        let root = TempDir::new().expect("temp dir");
        let bots = root.path().join("bots");
        let chat = ChannelChat::new(
            bots.clone(),
            root.path().join("channels"),
            Arc::new(ClaudeCli::new(bots, root.path().to_path_buf())),
        );

        assert!(matches!(
            chat.send_streaming("nao-existe", "oi", &mut Silent),
            Err(BotError::UnknownChannel)
        ));
        assert!(matches!(
            chat.send_streaming("nao-existe", "   ", &mut Silent),
            Err(BotError::InvalidMessage)
        ));
    }

    #[test]
    fn a_channel_that_is_answering_cannot_be_deleted_until_it_finishes() {
        let root = TempDir::new().expect("temp dir");
        let bots = root.path().join("bots");
        let chat = ChannelChat::new(
            bots.clone(),
            root.path().join("channels"),
            Arc::new(ClaudeCli::new(bots, root.path().to_path_buf())),
        );
        chat.create(NewChannel {
            name: "Infra".to_owned(),
            members: Vec::new(),
        })
        .expect("create");
        // Stands in for an answer that is still being written.
        chat.busy.lock().expect("lock").insert("infra".to_owned());

        assert!(matches!(chat.delete("infra"), Err(BotError::ChannelBusy)));
        assert!(matches!(
            chat.send_streaming("infra", "oi", &mut Silent),
            Err(BotError::ChannelBusy)
        ));
        chat.busy.lock().expect("lock").remove("infra");
        chat.delete("infra").expect("deleted once it has finished");
        assert!(chat.list().expect("list").is_empty());
    }

    /// Ignores everything: for the tests that only check what is refused.
    struct Silent;

    impl ChannelSink for Silent {
        fn text(&mut self, _author: &str, _piece: &str) {}
        fn status(&mut self, _phase: &str, _author: Option<&str>, _waiting: &[String]) {}
        fn message(&mut self, _message: &ChatMessage) {}
    }

    /// Records what the sink hears, in order.
    #[derive(Default)]
    struct Heard(Vec<String>);

    impl ChannelSink for Heard {
        fn text(&mut self, author: &str, _piece: &str) {
            self.0.push(format!("text {author}"));
        }
        fn status(&mut self, phase: &str, author: Option<&str>, _waiting: &[String]) {
            self.0
                .push(format!("status {phase} {}", author.unwrap_or("-")));
        }
        fn message(&mut self, message: &ChatMessage) {
            self.0.push(format!(
                "message {:?} {}",
                message.author,
                message.speaker.as_deref().unwrap_or("-")
            ));
        }
    }

    /// Runs the real Claude CLI with two members. Each answer comes in pieces, and a member's message is
    /// saved before the next member starts. It needs the CLI signed in, so it is ignored by default:
    /// `cargo test members_answer -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn members_answer_one_after_the_other_as_the_cli_writes_them() {
        use crate::bots::bot_folder::NewBot;

        let root = TempDir::new().expect("a temporary folder");
        let (bots, channels) = (root.path().join("bots"), root.path().join("channels"));
        let member = |name: &str| {
            bot_folder::create(
                &bots,
                NewBot {
                    name: name.to_owned(),
                    color: "sky".to_owned(),
                    provider: "claude".to_owned(),
                    model: None,
                    instructions: "Responda em uma frase curta, em português.".to_owned(),
                    team: None,
                },
            )
            .expect("the bot is made")
            .id
        };
        let members = vec![member("Ada"), member("Bento")];
        channel::create(
            &channels,
            &bots,
            NewChannel {
                name: "Teste".to_owned(),
                members,
            },
        )
        .expect("the channel is made");
        let chat = ChannelChat::new(
            bots.clone(),
            channels,
            Arc::new(ClaudeCli::new(bots, root.path().to_path_buf())),
        );

        let mut heard = Heard::default();
        chat.send_streaming("teste", "Olá, quem são vocês?", &mut heard)
            .expect("the message is sent");
        println!("{:#?}", heard.0);

        assert_eq!(heard.0.first().map(String::as_str), Some("message User -"));
        let answers: Vec<usize> = heard
            .0
            .iter()
            .enumerate()
            .filter(|(_, event)| event.starts_with("status answering"))
            .map(|(index, _)| index)
            .collect();
        if answers.len() == 2 {
            let first_saved = heard
                .0
                .iter()
                .position(|event| event.starts_with("message Bot"))
                .expect("the first member's message is saved");
            assert!(
                first_saved < answers[1],
                "the second member starts only after the first is saved"
            );
        }
        let pieces = heard
            .0
            .iter()
            .filter(|event| event.starts_with("text"))
            .count();
        assert!(pieces > 1, "an answer is written in more than one piece");
    }
}
