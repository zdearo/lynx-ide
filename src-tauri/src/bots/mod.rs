//! Bots: assistants that Lynx runs itself. They answer through Claude's CLI for now, and the ChatGPT
//! sign-in is in place for the next provider.

pub mod app_command_host;
pub mod base_instructions;
pub mod bot_access;
pub mod bot_bridge;
pub mod bot_chat;
pub mod bot_conversation;
pub mod bot_delete_impact;
pub mod bot_folder;
pub mod bot_mcp;
pub mod bot_memory;
pub mod bot_memory_capture;
pub mod bot_processes;
pub mod bot_recall;
pub mod bot_skills;
pub mod bot_thread;
pub mod bot_tools;
pub mod bot_topics;
pub mod bots_commands;
pub mod bots_events;
pub mod channel;
pub mod channel_chat;
pub mod chatgpt_credentials;
pub mod chatgpt_login;
pub mod chatgpt_protocol;
pub mod claude_cli;
pub mod command_permissions;
pub mod jev_decider;
pub mod jev_key;
pub mod turn_note;
