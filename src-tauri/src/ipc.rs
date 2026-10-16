//! Single registry of IPC commands and events. The TypeScript contract in
//! `src/bindings.ts` is generated from it; never edit that file by hand.

use tauri_specta::{Builder, collect_commands, collect_events};

use crate::app_info;
use crate::terminal::terminal_commands as terminal;
use crate::terminal::terminal_events::{TerminalExit, TerminalOutput};
use crate::workspace::workspace_commands as workspace;

pub fn builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            app_info::app_info,
            workspace::list_repos,
            workspace::add_repo,
            workspace::remove_repo,
            workspace::list_worktrees,
            workspace::create_worktree,
            workspace::remove_worktree,
            workspace::repo_refs,
            terminal::create_terminal,
            terminal::list_terminals,
            terminal::terminal_snapshot,
            terminal::write_terminal,
            terminal::resize_terminal,
            terminal::close_terminal,
        ])
        .events(collect_events![TerminalOutput, TerminalExit])
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
            "src/bindings.ts is out of date; run `pnpm bindings`"
        );
    }

    // Run through `pnpm bindings`; ignored so a plain `cargo test` never writes files.
    #[test]
    #[ignore = "writes src/bindings.ts"]
    fn write_bindings() {
        builder()
            .export(Typescript::default(), BINDINGS_PATH)
            .expect("export bindings");
    }
}
