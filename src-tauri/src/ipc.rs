//! Single registry of IPC commands and events. The TypeScript contract in
//! `src/bindings.ts` is generated from it; never edit that file by hand.

use tauri_specta::{Builder, collect_commands, collect_events};

use crate::agents::agent_commands as agents;
use crate::app_info;
use crate::changes::changes_commands as changes;
use crate::config::config_commands as config;
use crate::config::config_events::ConfigChanged;
use crate::files::files_commands as files;
use crate::pull_request::pull_request_commands as pull_request;
use crate::terminal::terminal_commands as terminal;
use crate::terminal::terminal_events::{AgentStatusChanged, TerminalExit, TerminalOutput};
use crate::workspace::workspace_commands as workspace;

pub fn builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            app_info::app_info,
            workspace::list_repos,
            workspace::add_repo,
            workspace::create_project,
            workspace::remove_repo,
            workspace::list_worktrees,
            workspace::create_worktree,
            workspace::remove_worktree,
            workspace::repo_refs,
            workspace::list_folders,
            workspace::create_folder,
            workspace::rename_folder,
            workspace::move_folder,
            workspace::delete_folder,
            workspace::set_folder_collapsed,
            workspace::move_repo,
            terminal::create_terminal,
            terminal::list_terminals,
            terminal::terminal_snapshot,
            terminal::write_terminal,
            terminal::resize_terminal,
            terminal::mark_agent_seen,
            terminal::close_terminal,
            agents::agent_catalog,
            agents::hook_status,
            agents::install_hooks,
            agents::remove_hooks,
            agents::write_agent_prompt,
            config::get_config,
            config::update_config,
            config::list_shells,
            config::open_config_file,
            files::list_directory,
            files::read_file,
            files::write_file,
            files::reveal_in_file_manager,
            changes::list_changes,
            changes::read_file_diff,
            changes::stage_change,
            changes::stage_changes,
            changes::discard_changes,
            changes::unstage_change,
            changes::commit_changes,
            changes::get_last_commit_message,
            changes::draft_commit_message,
            changes::get_branch_sync,
            changes::push_branch,
            changes::fetch_remote,
            changes::pull_branch,
            changes::force_push_branch,
            changes::list_commits,
            changes::list_commit_files,
            pull_request::get_pull_request,
            pull_request::open_pull_request_url,
            pull_request::get_check_log,
            pull_request::get_pull_request_discussion,
            pull_request::comment_on_pull_request,
            pull_request::reply_to_review_thread,
            pull_request::resolve_review_thread,
            pull_request::get_merge_methods,
            pull_request::run_pull_request_action,
            pull_request::get_github_account,
            pull_request::create_github_remote,
            pull_request::get_pull_request_form,
            pull_request::draft_pull_request_text,
            pull_request::create_pull_request,
        ])
        .events(collect_events![
            TerminalOutput,
            TerminalExit,
            AgentStatusChanged,
            ConfigChanged
        ])
}

#[cfg(test)]
mod tests {
    use specta_typescript::Typescript;

    use super::*;

    const BINDINGS_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../src/bindings.ts");

    // Fails when a command or type changes in Rust without regenerating the bindings.
    #[test]
    fn committed_bindings_match_rust_contract() {
        let dir = tempfile::tempdir().expect("temp dir");
        let generated_path = dir.path().join("bindings.ts");
        builder()
            .export(Typescript::default(), &generated_path)
            .expect("export bindings");

        let generated = std::fs::read_to_string(generated_path).expect("read generated");
        let committed = std::fs::read_to_string(BINDINGS_PATH).unwrap_or_default();
        assert_eq!(
            generated, committed,
            "src/bindings.ts is out of date; run `bun run bindings`"
        );
    }

    // Run through `bun run bindings`; ignored so a plain `cargo test` never writes files.
    #[test]
    #[ignore = "writes src/bindings.ts"]
    fn write_bindings() {
        builder()
            .export(Typescript::default(), BINDINGS_PATH)
            .expect("export bindings");
    }
}
