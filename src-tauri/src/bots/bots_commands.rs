//! Thin commands for the bots: their folders, their conversations, the ChatGPT sign-in and the Claude
//! test. The work sits in the domain modules.

use std::path::Path;
use std::sync::Arc;

use tauri::{AppHandle, State};
use tauri_specta::Event;

use crate::error::{AppError, IpcError};
use crate::orchestration::service::{Orchestration, WorkerRow};
use crate::process::blocking_task::run_blocking;
use crate::team::team_events::TeamChanged;
use crate::team::team_service::TeamService;

use super::bot_access::{AccessState, Grant, RegisteredRepos, RequestKind};
use super::bot_chat::{BotChat, BotReply};
use super::bot_conversation::{Author, ChatMessage};
use super::bot_delete_impact::{DeleteImpact, delete_impact};
use super::bot_folder::{self, BotDetail, BotError, BotSetup, BotSummary, BotsRoot, NewBot};
use super::bot_mcp::{self, BotMcpServer};
use super::bot_memory::{self, MemoryView};
use super::bot_processes::{BotProcesses, RunningCommand};
use super::bot_skills::{self, BotSkill};
use super::bot_thread::{self, ThreadSummary};
use super::bot_tools::Announce;
use super::bots_events::{BotReplyDelta, ChannelStatus, ChatMessageAdded};
use super::channel::{ChannelSummary, NewChannel};
use super::channel_chat::{ChannelChat, ChannelSink};
use super::chatgpt_login::{ChatGptLogin, ChatGptStatus};
use super::command_permissions;
use super::jev_decider::{self, JevTest};
use super::jev_key;

/// Every bot in the bots folder, by name.
#[tauri::command]
#[specta::specta]
pub async fn bots_list(root: State<'_, Arc<BotsRoot>>) -> Result<Vec<BotSummary>, IpcError> {
    let root = Arc::clone(root.inner());
    run_blocking(move || Ok::<_, AppError>(bot_folder::list(&root.0)?)).await
}

/// Makes a bot from the form. Its folder and files are written before the answer comes back.
#[tauri::command]
#[specta::specta]
pub async fn bots_create(
    root: State<'_, Arc<BotsRoot>>,
    bot: NewBot,
) -> Result<BotSummary, IpcError> {
    let root = Arc::clone(root.inner());
    run_blocking(move || Ok::<_, AppError>(bot_folder::create(&root.0, bot)?)).await
}

/// Replaces what a bot is told to do. Its next answer already uses the new instructions.
#[tauri::command]
#[specta::specta]
pub async fn bots_update_instructions(
    root: State<'_, Arc<BotsRoot>>,
    id: String,
    instructions: String,
) -> Result<BotDetail, IpcError> {
    let root = Arc::clone(root.inner());
    run_blocking(move || {
        Ok::<_, AppError>(bot_folder::update_instructions(
            &root.0,
            &id,
            &instructions,
        )?)
    })
    .await
}

/// Changes a bot's engine, model and team. Its name, colour and instructions stay as they are.
#[tauri::command]
#[specta::specta]
pub async fn bots_update_setup(
    root: State<'_, Arc<BotsRoot>>,
    id: String,
    setup: BotSetup,
) -> Result<BotDetail, IpcError> {
    let root = Arc::clone(root.inner());
    run_blocking(move || Ok::<_, AppError>(bot_folder::update_setup(&root.0, &id, setup)?)).await
}

/// Whether a Jev key is saved. The key itself never leaves the backend.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct JevStatus {
    pub configured: bool,
}

fn jev_key_file(root: &BotsRoot) -> std::path::PathBuf {
    root.0.join("jev.key")
}

fn jev_status_of(root: &BotsRoot) -> JevStatus {
    JevStatus {
        configured: jev_key::read(&jev_key_file(root)).is_some(),
    }
}

/// Whether a Jev key is saved.
#[tauri::command]
#[specta::specta]
pub async fn bots_jev_status(root: State<'_, Arc<BotsRoot>>) -> Result<JevStatus, IpcError> {
    let root = Arc::clone(root.inner());
    run_blocking(move || Ok::<_, AppError>(jev_status_of(&root))).await
}

/// Saves the Jev key, replacing the old one.
#[tauri::command]
#[specta::specta]
pub async fn bots_jev_save_key(
    root: State<'_, Arc<BotsRoot>>,
    key: String,
) -> Result<JevStatus, IpcError> {
    let root = Arc::clone(root.inner());
    run_blocking(move || {
        jev_key::save(&jev_key_file(&root), &key)?;
        Ok::<_, AppError>(jev_status_of(&root))
    })
    .await
}

/// Forgets the Jev key.
#[tauri::command]
#[specta::specta]
pub async fn bots_jev_clear_key(root: State<'_, Arc<BotsRoot>>) -> Result<JevStatus, IpcError> {
    let root = Arc::clone(root.inner());
    run_blocking(move || {
        jev_key::clear(&jev_key_file(&root)).map_err(BotError::from)?;
        Ok::<_, AppError>(jev_status_of(&root))
    })
    .await
}

/// Asks Jev a fixed question with the saved key, to show whether it works and how long it takes.
#[tauri::command]
#[specta::specta]
pub async fn bots_jev_test(root: State<'_, Arc<BotsRoot>>) -> Result<JevTest, IpcError> {
    let root = Arc::clone(root.inner());
    run_blocking(move || Ok::<_, AppError>(jev_decider::test(&jev_key_file(&root)))).await
}

/// The command patterns a bot may run, for the details tab.
#[tauri::command]
#[specta::specta]
pub async fn bots_allowed_commands(
    root: State<'_, Arc<BotsRoot>>,
    id: String,
) -> Result<Vec<String>, IpcError> {
    let root = Arc::clone(root.inner());
    run_blocking(move || Ok::<_, AppError>(command_permissions::allowed(&root.0, &id)?)).await
}

/// Lets a bot run the command patterns, one per entry. Returns the patterns after it.
#[tauri::command]
#[specta::specta]
pub async fn bots_allow_commands(
    root: State<'_, Arc<BotsRoot>>,
    id: String,
    patterns: Vec<String>,
) -> Result<Vec<String>, IpcError> {
    let root = Arc::clone(root.inner());
    run_blocking(move || Ok::<_, AppError>(command_permissions::allow(&root.0, &id, &patterns)?))
        .await
}

/// Takes a command pattern back from a bot. Returns the patterns after it.
#[tauri::command]
#[specta::specta]
pub async fn bots_disallow_command(
    root: State<'_, Arc<BotsRoot>>,
    id: String,
    pattern: String,
) -> Result<Vec<String>, IpcError> {
    let root = Arc::clone(root.inner());
    run_blocking(move || Ok::<_, AppError>(command_permissions::remove(&root.0, &id, &pattern)?))
        .await
}

/// The commands a bot left running, and the end of what each printed, for the Agents tab.
#[tauri::command]
#[specta::specta]
pub async fn bots_running_commands(
    processes: State<'_, Arc<BotProcesses>>,
    id: String,
) -> Result<Vec<RunningCommand>, IpcError> {
    let processes = Arc::clone(processes.inner());
    run_blocking(move || Ok::<_, AppError>(processes.status(&id))).await
}

/// Stops a command a bot left running.
#[tauri::command]
#[specta::specta]
pub async fn bots_stop_command(
    processes: State<'_, Arc<BotProcesses>>,
    id: String,
    session: String,
) -> Result<(), IpcError> {
    let processes = Arc::clone(processes.inner());
    run_blocking(move || Ok::<_, AppError>(processes.stop(&id, &session)?)).await
}

/// The bot's memory, and how much of it loads each turn.
#[tauri::command]
#[specta::specta]
pub async fn bots_memory(
    root: State<'_, Arc<BotsRoot>>,
    id: String,
) -> Result<MemoryView, IpcError> {
    let root = Arc::clone(root.inner());
    run_blocking(move || {
        let dir = bot_folder::folder_of(&root.0, &id)?;
        Ok::<_, AppError>(bot_memory::view(&dir))
    })
    .await
}

/// Replaces the bot's memory with what the person wrote.
#[tauri::command]
#[specta::specta]
pub async fn bots_memory_save(
    root: State<'_, Arc<BotsRoot>>,
    id: String,
    text: String,
) -> Result<MemoryView, IpcError> {
    let root = Arc::clone(root.inner());
    run_blocking(move || {
        let dir = bot_folder::folder_of(&root.0, &id)?;
        Ok::<_, AppError>(bot_memory::save(&dir, &text)?)
    })
    .await
}

/// The skills a bot has, for the details tab.
#[tauri::command]
#[specta::specta]
pub async fn bots_skills(
    root: State<'_, Arc<BotsRoot>>,
    id: String,
) -> Result<Vec<BotSkill>, IpcError> {
    let root = Arc::clone(root.inner());
    run_blocking(move || {
        let dir = bot_folder::folder_of(&root.0, &id)?;
        Ok::<_, AppError>(bot_skills::list(&dir))
    })
    .await
}

/// Copies a skill folder the person picked into the bot. Returns the bot's skills after it.
#[tauri::command]
#[specta::specta]
pub async fn bots_skill_add(
    root: State<'_, Arc<BotsRoot>>,
    id: String,
    path: String,
) -> Result<Vec<BotSkill>, IpcError> {
    let root = Arc::clone(root.inner());
    run_blocking(move || {
        let dir = bot_folder::folder_of(&root.0, &id)?;
        Ok::<_, AppError>(bot_skills::add(&dir, Path::new(&path))?)
    })
    .await
}

/// Takes a skill out of the bot. Returns the bot's skills after it.
#[tauri::command]
#[specta::specta]
pub async fn bots_skill_remove(
    root: State<'_, Arc<BotsRoot>>,
    id: String,
    name: String,
) -> Result<Vec<BotSkill>, IpcError> {
    let root = Arc::clone(root.inner());
    run_blocking(move || {
        let dir = bot_folder::folder_of(&root.0, &id)?;
        Ok::<_, AppError>(bot_skills::remove(&dir, &name)?)
    })
    .await
}

/// The MCP servers a bot has, by name and where each runs. Their secrets are not listed.
#[tauri::command]
#[specta::specta]
pub async fn bots_mcp_servers(
    root: State<'_, Arc<BotsRoot>>,
    id: String,
) -> Result<Vec<BotMcpServer>, IpcError> {
    let root = Arc::clone(root.inner());
    run_blocking(move || {
        let dir = bot_folder::folder_of(&root.0, &id)?;
        Ok::<_, AppError>(bot_mcp::list(&dir)?)
    })
    .await
}

/// Adds the MCP servers in a `{"mcpServers": {...}}` config to a bot. Returns the bot's servers after it.
#[tauri::command]
#[specta::specta]
pub async fn bots_mcp_add(
    root: State<'_, Arc<BotsRoot>>,
    id: String,
    config: String,
) -> Result<Vec<BotMcpServer>, IpcError> {
    let root = Arc::clone(root.inner());
    run_blocking(move || {
        let dir = bot_folder::folder_of(&root.0, &id)?;
        Ok::<_, AppError>(bot_mcp::add(&dir, &config)?)
    })
    .await
}

/// Takes an MCP server out of a bot. Returns the bot's servers after it.
#[tauri::command]
#[specta::specta]
pub async fn bots_mcp_remove(
    root: State<'_, Arc<BotsRoot>>,
    id: String,
    name: String,
) -> Result<Vec<BotMcpServer>, IpcError> {
    let root = Arc::clone(root.inner());
    run_blocking(move || {
        let dir = bot_folder::folder_of(&root.0, &id)?;
        Ok::<_, AppError>(bot_mcp::remove(&dir, &name)?)
    })
    .await
}

/// Replaces a channel's members, and returns the channel as it now is.
#[tauri::command]
#[specta::specta]
pub async fn channels_update_members(
    chat: State<'_, Arc<ChannelChat>>,
    id: String,
    members: Vec<String>,
) -> Result<ChannelSummary, IpcError> {
    let chat = Arc::clone(chat.inner());
    run_blocking(move || Ok::<_, AppError>(chat.update_members(&id, members)?)).await
}

/// Removes a bot with its conversation. Refused while the bot is answering.
#[tauri::command]
#[specta::specta]
pub async fn bots_delete(
    app: AppHandle,
    chat: State<'_, Arc<BotChat>>,
    processes: State<'_, Arc<BotProcesses>>,
    team: State<'_, Arc<TeamService>>,
    orchestration: State<'_, Arc<Orchestration>>,
    id: String,
) -> Result<(), IpcError> {
    let chat = Arc::clone(chat.inner());
    let processes = Arc::clone(processes.inner());
    let team = Arc::clone(team.inner());
    let orchestration = Arc::clone(orchestration.inner());
    let team_changed = run_blocking(move || {
        Ok::<_, AppError>(delete_bot(&chat, &processes, &team, &orchestration, &id)?)
    })
    .await?;
    if team_changed {
        let _ = TeamChanged {}.emit(&app);
    }
    Ok(())
}

/// What deleting a bot takes with it, for the confirmation. Changes nothing.
#[tauri::command]
#[specta::specta]
pub async fn bots_delete_impact(
    team: State<'_, Arc<TeamService>>,
    orchestration: State<'_, Arc<Orchestration>>,
    id: String,
) -> Result<DeleteImpact, IpcError> {
    let team = Arc::clone(team.inner());
    let orchestration = Arc::clone(orchestration.inner());
    run_blocking(move || delete_impact(&team, &orchestration, &id)).await
}

/// The folder goes first, so a bot that is answering is refused before the team is touched. Once the folder is
/// gone the bot is gone, so a team that fails to drop it is logged and the delete still succeeds. Returns
/// whether the team lost a member.
fn delete_bot(
    chat: &BotChat,
    processes: &BotProcesses,
    team: &TeamService,
    orchestration: &Orchestration,
    id: &str,
) -> Result<bool, BotError> {
    chat.delete(id)?;
    // What the bot left running goes with it.
    processes.stop_all(id);
    // The workers it started stop with it; their members leave the team with the bot. Best effort, as the
    // team is: the bot is deleted already.
    if let Err(error) = orchestration.stop_workers_of_bot(id) {
        log::error!("could not stop the workers of bot {id}: {error}");
    }
    match team.remove_bot(id) {
        Ok(removed) => Ok(removed > 0),
        Err(error) => {
            log::error!("could not remove bot {id} from the team: {error}");
            Ok(false)
        }
    }
}

/// How a bot's own tools show a message they saved: as a chat event, so the chat shows it at once.
pub fn announce_message(app: AppHandle) -> Announce {
    Arc::new(move |thread: &str, message: &ChatMessage| {
        let _ = ChatMessageAdded {
            thread: thread.to_owned(),
            message: message.clone(),
        }
        .emit(&app);
    })
}

/// The repositories a bot may read, as the person granted them.
#[tauri::command]
#[specta::specta]
pub async fn bots_grants(
    root: State<'_, Arc<BotsRoot>>,
    id: String,
) -> Result<Vec<Grant>, IpcError> {
    let root = Arc::clone(root.inner());
    run_blocking(move || Ok::<_, AppError>(bot_folder::grants(&root.0, &id)?)).await
}

/// Approves a bot's request to read a repository: the bot may read it from now on, and it is woken to
/// go on with its task. Refused while the bot is answering; the grant then applies to its next answer.
#[tauri::command]
#[specta::specta]
pub async fn bots_approve_access(
    app: AppHandle,
    chat: State<'_, Arc<BotChat>>,
    root: State<'_, Arc<BotsRoot>>,
    repos: State<'_, Arc<dyn RegisteredRepos>>,
    id: String,
    thread: String,
    message_id: String,
) -> Result<BotReply, IpcError> {
    let chat = Arc::clone(chat.inner());
    let root = Arc::clone(root.inner());
    let repos = Arc::clone(repos.inner());
    run_blocking(move || {
        let asked = chat.pending_request(&id, &thread, &message_id)?;
        let note = match asked.kind {
            RequestKind::Project => {
                let repo_id = asked.repo_id.clone().unwrap_or_default();
                if !repos.repos().iter().any(|repo| repo.id == repo_id) {
                    return Err(AppError::Bot(bot_folder::BotError::UnknownRepo));
                }
                let name = asked.items.first().cloned().unwrap_or_default();
                bot_folder::grant(
                    &root.0,
                    &id,
                    Grant {
                        id: repo_id,
                        name: name.clone(),
                    },
                )?;
                format!("A pessoa aprovou seu acesso a {name}. Retome a tarefa de onde parou.")
            }
            RequestKind::Workers => {
                let repo_id = asked.repo_id.clone().unwrap_or_default();
                if !repos.repos().iter().any(|repo| repo.id == repo_id) {
                    return Err(AppError::Bot(bot_folder::BotError::UnknownRepo));
                }
                let name = asked.items.first().cloned().unwrap_or_default();
                bot_folder::grant_workers(
                    &root.0,
                    &id,
                    Grant {
                        id: repo_id,
                        name: name.clone(),
                    },
                )?;
                format!(
                    "A pessoa liberou você a lançar agentes em {name}. Eles rodam sem pedir permissão. Retome a tarefa de onde parou."
                )
            }
            RequestKind::Commands => {
                command_permissions::allow(&root.0, &id, &asked.items)?;
                format!(
                    "A pessoa liberou estes comandos para você: {}. Retome a tarefa de onde parou.",
                    asked.items.join(", ")
                )
            }
        };
        chat.decide_request(&id, &thread, &message_id, AccessState::Approved)?;
        let key = bot_thread::key_of(&id, &thread);
        let mut on_text = |piece: &str| {
            let _ = BotReplyDelta {
                thread: key.clone(),
                author: id.clone(),
                text: piece.to_owned(),
            }
            .emit(&app);
        };
        let message = chat.resume(&id, &thread, &note, &mut on_text)?;
        BotChat::kick(Arc::clone(&chat), id.clone(), thread.clone());
        Ok::<_, AppError>(BotReply {
            ok: message.author == Author::Bot,
            text: message.text,
        })
    })
    .await
}

/// Refuses a bot's request to read a repository. The bot sees the refusal in the conversation.
#[tauri::command]
#[specta::specta]
pub async fn bots_deny_access(
    chat: State<'_, Arc<BotChat>>,
    id: String,
    thread: String,
    message_id: String,
) -> Result<(), IpcError> {
    let chat = Arc::clone(chat.inner());
    run_blocking(move || {
        chat.decide_request(&id, &thread, &message_id, AccessState::Denied)?;
        Ok::<_, AppError>(())
    })
    .await
}

/// The repositories where a bot may start agents.
#[tauri::command]
#[specta::specta]
pub async fn bots_worker_grants(
    root: State<'_, Arc<BotsRoot>>,
    id: String,
) -> Result<Vec<Grant>, IpcError> {
    let root = Arc::clone(root.inner());
    run_blocking(move || Ok::<_, AppError>(bot_folder::worker_grants(&root.0, &id)?)).await
}

/// Takes back the right to start agents in a repository. Returns where the bot still may.
#[tauri::command]
#[specta::specta]
pub async fn bots_revoke_workers(
    root: State<'_, Arc<BotsRoot>>,
    id: String,
    repo_id: String,
) -> Result<Vec<Grant>, IpcError> {
    let root = Arc::clone(root.inner());
    run_blocking(move || {
        bot_folder::revoke_workers(&root.0, &id, &repo_id)?;
        Ok::<_, AppError>(bot_folder::worker_grants(&root.0, &id)?)
    })
    .await
}

/// Takes back a repository a bot could read. Returns what it can still read.
#[tauri::command]
#[specta::specta]
pub async fn bots_revoke_access(
    root: State<'_, Arc<BotsRoot>>,
    id: String,
    repo_id: String,
) -> Result<Vec<Grant>, IpcError> {
    let root = Arc::clone(root.inner());
    run_blocking(move || {
        bot_folder::revoke(&root.0, &id, &repo_id)?;
        Ok::<_, AppError>(bot_folder::grants(&root.0, &id)?)
    })
    .await
}

/// The workers a bot started, and whether each still runs, for the bot's agents tab.
#[tauri::command]
#[specta::specta]
pub async fn bots_workers(
    orchestration: State<'_, Arc<Orchestration>>,
    id: String,
) -> Result<Vec<WorkerRow>, IpcError> {
    let orchestration = Arc::clone(orchestration.inner());
    run_blocking(move || Ok::<_, AppError>(orchestration.workers_of_bot(&id)?)).await
}

/// One bot's settings, instructions and folder, for the details tab.
#[tauri::command]
#[specta::specta]
pub async fn bots_get(root: State<'_, Arc<BotsRoot>>, id: String) -> Result<BotDetail, IpcError> {
    let root = Arc::clone(root.inner());
    run_blocking(move || Ok::<_, AppError>(bot_folder::detail(&root.0, &id)?)).await
}

/// One thread of a bot, oldest message first.
#[tauri::command]
#[specta::specta]
pub async fn bots_conversation(
    chat: State<'_, Arc<BotChat>>,
    id: String,
    thread: String,
) -> Result<Vec<ChatMessage>, IpcError> {
    let chat = Arc::clone(chat.inner());
    run_blocking(move || Ok::<_, AppError>(chat.history(&id, &thread)?)).await
}

/// The threads of a bot, the most recently active first. A bot with none gets its first.
#[tauri::command]
#[specta::specta]
pub async fn bots_threads(
    chat: State<'_, Arc<BotChat>>,
    id: String,
) -> Result<Vec<ThreadSummary>, IpcError> {
    let chat = Arc::clone(chat.inner());
    run_blocking(move || Ok::<_, AppError>(chat.threads(&id)?)).await
}

/// Starts a new thread with a bot.
#[tauri::command]
#[specta::specta]
pub async fn bots_thread_create(
    chat: State<'_, Arc<BotChat>>,
    id: String,
) -> Result<ThreadSummary, IpcError> {
    let chat = Arc::clone(chat.inner());
    run_blocking(move || Ok::<_, AppError>(chat.create_thread(&id)?)).await
}

/// Gives a thread a title; a blank one makes it untitled again.
#[tauri::command]
#[specta::specta]
pub async fn bots_thread_rename(
    chat: State<'_, Arc<BotChat>>,
    id: String,
    thread: String,
    title: String,
) -> Result<ThreadSummary, IpcError> {
    let chat = Arc::clone(chat.inner());
    run_blocking(move || Ok::<_, AppError>(chat.rename_thread(&id, &thread, &title)?)).await
}

/// Removes a thread with its conversation. Refused while the bot is answering.
#[tauri::command]
#[specta::specta]
pub async fn bots_thread_delete(
    chat: State<'_, Arc<BotChat>>,
    id: String,
    thread: String,
) -> Result<(), IpcError> {
    let chat = Arc::clone(chat.inner());
    run_blocking(move || Ok::<_, AppError>(chat.delete_thread(&id, &thread)?)).await
}

/// Stops the answer a bot is writing in a thread. What it had written stays there, marked as stopped.
#[tauri::command]
#[specta::specta]
pub async fn bots_stop(
    chat: State<'_, Arc<BotChat>>,
    id: String,
    thread: String,
) -> Result<bool, IpcError> {
    let chat = Arc::clone(chat.inner());
    run_blocking(move || Ok::<_, AppError>(chat.stop(&id, &thread))).await
}

/// Stops the answer of a channel: who is writing, and who was waiting. What was written stays.
#[tauri::command]
#[specta::specta]
pub async fn channels_stop(
    chat: State<'_, Arc<ChannelChat>>,
    id: String,
) -> Result<bool, IpcError> {
    let chat = Arc::clone(chat.inner());
    run_blocking(move || Ok::<_, AppError>(chat.stop(&id))).await
}

/// Every channel, by name.
#[tauri::command]
#[specta::specta]
pub async fn channels_list(
    chat: State<'_, Arc<ChannelChat>>,
) -> Result<Vec<ChannelSummary>, IpcError> {
    let chat = Arc::clone(chat.inner());
    run_blocking(move || Ok::<_, AppError>(chat.list()?)).await
}

/// Makes a channel with its members.
#[tauri::command]
#[specta::specta]
pub async fn channels_create(
    chat: State<'_, Arc<ChannelChat>>,
    channel: NewChannel,
) -> Result<ChannelSummary, IpcError> {
    let chat = Arc::clone(chat.inner());
    run_blocking(move || Ok::<_, AppError>(chat.create(channel)?)).await
}

/// Removes a channel with its conversation. Refused while the channel is answering.
#[tauri::command]
#[specta::specta]
pub async fn channels_delete(
    chat: State<'_, Arc<ChannelChat>>,
    id: String,
) -> Result<(), IpcError> {
    let chat = Arc::clone(chat.inner());
    run_blocking(move || Ok::<_, AppError>(chat.delete(&id)?)).await
}

/// The conversation of a channel, oldest message first.
#[tauri::command]
#[specta::specta]
pub async fn channels_conversation(
    chat: State<'_, Arc<ChannelChat>>,
    id: String,
) -> Result<Vec<ChatMessage>, IpcError> {
    let chat = Arc::clone(chat.inner());
    run_blocking(move || Ok::<_, AppError>(chat.history(&id)?)).await
}

/// Sends the person's message to a channel. While the members answer, the channel's events go out as
/// they happen: each message saved, the status of the answer, and the pieces of each reply.
#[tauri::command]
#[specta::specta]
pub async fn channels_chat(
    app: AppHandle,
    chat: State<'_, Arc<ChannelChat>>,
    id: String,
    text: String,
) -> Result<(), IpcError> {
    let chat = Arc::clone(chat.inner());
    run_blocking(move || {
        let mut events = ChannelEvents {
            thread: format!("channel:{id}"),
            app,
        };
        Ok::<_, AppError>(chat.send_streaming(&id, &text, &mut events)?)
    })
    .await
}

/// Turns what a channel's send reports into the events the screen listens to.
struct ChannelEvents {
    thread: String,
    app: AppHandle,
}

impl ChannelSink for ChannelEvents {
    fn text(&mut self, author: &str, piece: &str) {
        let _ = BotReplyDelta {
            thread: self.thread.clone(),
            author: author.to_owned(),
            text: piece.to_owned(),
        }
        .emit(&self.app);
    }

    fn status(&mut self, phase: &str, author: Option<&str>, waiting: &[String]) {
        let _ = ChannelStatus {
            thread: self.thread.clone(),
            phase: phase.to_owned(),
            author: author.map(str::to_owned),
            waiting: waiting.to_vec(),
        }
        .emit(&self.app);
    }

    fn message(&mut self, message: &ChatMessage) {
        let _ = ChatMessageAdded {
            thread: self.thread.clone(),
            message: message.clone(),
        }
        .emit(&self.app);
    }
}

/// Sends the user's message to a bot and waits for its answer. Each piece of the answer goes out as a
/// `BotReplyDelta` event while it is written. Off the main thread: an answer takes seconds.
#[tauri::command]
#[specta::specta]
pub async fn bots_chat(
    app: AppHandle,
    chat: State<'_, Arc<BotChat>>,
    id: String,
    thread: String,
    text: String,
) -> Result<BotReply, IpcError> {
    let chat = Arc::clone(chat.inner());
    run_blocking(move || {
        let key = bot_thread::key_of(&id, &thread);
        let mut on_text = |piece: &str| {
            let _ = BotReplyDelta {
                thread: key.clone(),
                author: id.clone(),
                text: piece.to_owned(),
            }
            .emit(&app);
        };
        let reply = chat.send_streaming(&id, &thread, &text, &mut on_text)?;
        // News that came while the thread answered is read now that it is free.
        BotChat::kick(Arc::clone(&chat), id.clone(), thread.clone());
        BotChat::after_turn(Arc::clone(&chat), id.clone(), thread.clone());
        Ok::<_, AppError>(reply)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub fn bots_chatgpt_status(login: State<'_, Arc<ChatGptLogin>>) -> ChatGptStatus {
    login.status()
}

/// Opens the browser and returns at once. The outcome comes later as an event.
#[tauri::command]
#[specta::specta]
pub fn bots_chatgpt_sign_in(login: State<'_, Arc<ChatGptLogin>>) -> ChatGptStatus {
    login.inner().sign_in()
}

/// Ends the session at OpenAI, a network call, so it runs off the main thread.
#[tauri::command]
#[specta::specta]
pub async fn bots_chatgpt_sign_out(
    login: State<'_, Arc<ChatGptLogin>>,
) -> Result<ChatGptStatus, IpcError> {
    let login = Arc::clone(login.inner());
    run_blocking(move || Ok::<_, AppError>(login.sign_out())).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::harness::HarnessId;
    use crate::bots::bot_processes::CommandHost;
    use crate::bots::claude_cli::ClaudeCli;
    use crate::orchestration::service_tests::FakeHost;
    use crate::persistence::AppDatabase;
    use crate::team::team_error::TeamError;
    use crate::team::team_model::NewTeamMember;
    use crate::team::team_model::{MessageKind, TeamMessage};
    use tempfile::TempDir;

    /// Bots here run no commands, so no terminal is ever asked for one.
    struct NoTerminals;

    impl CommandHost for NoTerminals {
        fn start(&self, _cwd: &Path, _command: &str) -> Result<String, String> {
            Err("no terminals in tests".to_owned())
        }
        fn running(&self, _session: &str) -> bool {
            false
        }
        fn output(&self, _session: &str) -> String {
            String::new()
        }
        fn stop(&self, _session: &str) {}
    }

    /// The team as the app builds it: a bot joins only while its folder is under `bots`.
    fn team_over(database: &Arc<AppDatabase>, bots: &Path) -> Arc<TeamService> {
        let bots = bots.to_path_buf();
        Arc::new(TeamService::new(
            Arc::clone(database),
            Box::new(move |bot_id: &str| bot_folder::folder_of(&bots, bot_id).is_ok()),
        ))
    }

    /// The docs bot joining the team of `/repo`.
    fn docs_joins() -> NewTeamMember {
        NewTeamMember::Bot {
            bot_id: "docs".to_owned(),
            role: "Docs".to_owned(),
            worktree: "/repo".to_owned(),
        }
    }

    /// The orchestration over `team`. Deleting a bot never starts a worker, so the host is never asked.
    fn orchestration_over(team: &Arc<TeamService>) -> Orchestration {
        Orchestration::new(Arc::clone(team), Arc::new(FakeHost::new()))
    }

    /// The bot's parts over one database file in `dir`, where the bot folder `docs` exists.
    fn parts(dir: &TempDir) -> (BotChat, BotProcesses, Arc<AppDatabase>) {
        let bots = dir.path().join("bots");
        bot_folder::BotFolder::open(&bots, "docs").expect("the folder opens");
        let claude = Arc::new(ClaudeCli::new(bots.clone(), dir.path().to_path_buf()));
        let chat = BotChat::new(bots.clone(), claude);
        let processes = BotProcesses::new(Arc::new(NoTerminals), bots);
        let database = Arc::new(AppDatabase::open(&dir.path().join("app.db")).expect("open"));
        (chat, processes, database)
    }

    #[test]
    fn deleting_a_bot_removes_its_folder_and_its_place_in_the_team() {
        let dir = TempDir::new().expect("a temporary folder");
        let (chat, processes, database) = parts(&dir);
        let team = team_over(&database, &dir.path().join("bots"));
        team.add_member(docs_joins()).expect("join");

        assert!(
            delete_bot(&chat, &processes, &team, &orchestration_over(&team), "docs")
                .expect("delete")
        );

        assert!(!dir.path().join("bots").join("docs").exists());
        assert!(team.snapshot().expect("snapshot").members.is_empty());
    }

    #[test]
    fn a_deleted_bot_cannot_join_a_team_again() {
        let dir = TempDir::new().expect("a temporary folder");
        let (chat, processes, database) = parts(&dir);
        let team = team_over(&database, &dir.path().join("bots"));
        team.add_member(docs_joins()).expect("join");
        delete_bot(&chat, &processes, &team, &orchestration_over(&team), "docs").expect("delete");

        let joined = team.add_member(docs_joins());

        assert!(matches!(joined, Err(TeamError::UnknownBot)));
        assert!(team.snapshot().expect("snapshot").members.is_empty());
    }

    #[test]
    fn a_team_that_cannot_drop_the_bot_does_not_undo_its_deletion() {
        let dir = TempDir::new().expect("a temporary folder");
        let (chat, processes, database) = parts(&dir);
        let team = team_over(&database, &dir.path().join("bots"));
        team.add_member(docs_joins()).expect("join");
        // The team's table is gone, so its change fails.
        database
            .lock()
            .execute_batch("DROP TABLE team_members")
            .expect("drop");

        assert!(
            !delete_bot(&chat, &processes, &team, &orchestration_over(&team), "docs")
                .expect("delete")
        );

        assert!(!dir.path().join("bots").join("docs").exists());
    }

    #[test]
    fn deleting_a_bot_takes_its_workers_and_their_messages_with_it() {
        let dir = TempDir::new().expect("a temporary folder");
        let (chat, processes, database) = parts(&dir);
        let team = team_over(&database, &dir.path().join("bots"));
        let bot = team.add_member(docs_joins()).expect("join");
        let worker = team
            .add_member(NewTeamMember::Agent {
                role: "Writer".to_owned(),
                harness: Some(HarnessId::Claude),
                parent_id: Some(bot.id.clone()),
                orchestrator: false,
                task: "Audite".to_owned(),
                acceptance: String::new(),
                depends_on: Vec::new(),
                worktree: "/repo".to_owned(),
                session_id: None,
            })
            .expect("a member under the bot");
        team.add_message(&TeamMessage {
            id: "message-docs".to_owned(),
            member_id: worker.id.clone(),
            kind: MessageKind::Done,
            body: "Feito".to_owned(),
            created_at: 1,
        })
        .expect("message");

        delete_bot(&chat, &processes, &team, &orchestration_over(&team), "docs").expect("delete");

        assert!(team.snapshot().expect("snapshot").members.is_empty());
        assert!(
            database.messages_of(&worker.id).expect("read").is_empty(),
            "the messages of the bot's workers go with them"
        );
    }
}
