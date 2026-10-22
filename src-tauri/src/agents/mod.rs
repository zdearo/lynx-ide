//! Agent sessions: which harness a terminal runs, and what that agent is doing right now.

pub mod activity;
pub mod agent_catalog;
pub mod agent_commands;
pub mod agent_monitor;
pub mod agent_prompt_file;
pub mod agent_title;
pub mod claude_hooks;
pub mod codex_hooks;
pub mod harness;
pub mod hook_events;
pub mod hook_hub;
pub mod hook_install;
pub mod hook_relay;
pub mod hook_targets;
pub mod hub_address;
pub mod json_hook_file;
pub mod mcp_install;
pub mod mcp_server;
pub mod omp_hooks;
pub mod opencode_hooks;
pub mod plugin_file;
pub mod title_scanner;
