//! The Windows foreground on a real console: a stand-in CLI named like an agent is started from
//! cmd.exe and must be followed until it is interrupted. Windows-only, like the Unix shell tests.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::agents::activity::AgentStatus;
use crate::agents::harness::HarnessId;
use crate::agents::hook_hub::HookHub;
use crate::browser::SessionOwners;

use super::Shell;
use super::terminal_events::TerminalEventSink;
use super::terminal_manager::TerminalManager;
use super::terminal_test_support::{RecordingSink, answer_cursor_request, path_of};

/// A copy of PING.EXE named `claude.exe`. It runs for a while and stops on Ctrl+C, like an agent.
fn fake_cli(dir: &Path) -> PathBuf {
    let bin = dir.join("bin");
    fs::create_dir_all(&bin).expect("bin dir");
    let target = bin.join("claude.exe");
    let system = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
    fs::copy(
        PathBuf::from(system).join("System32").join("PING.EXE"),
        &target,
    )
    .expect("copy ping");
    target
}

#[test]
fn an_agent_typed_into_cmd_is_followed_until_it_is_interrupted() {
    let sink = Arc::new(RecordingSink::default());
    let shell = Arc::new(|| Shell {
        program: "cmd.exe".to_owned(),
        args: Vec::new(),
    });
    let manager = TerminalManager::new(
        shell,
        Arc::clone(&sink) as Arc<dyn TerminalEventSink>,
        HookHub::disabled(),
        SessionOwners::default(),
        None,
        None,
    );
    let dir = tempfile::tempdir().expect("temp dir");
    let cli = fake_cli(dir.path());
    let info = manager
        .create(&path_of(&dir), None, 80, 24)
        .expect("create");
    assert_eq!(info.harness, None, "a new shell is not an agent");

    answer_cursor_request(&sink, &manager, &info.id);

    // Quoted, because the user's folder can hold a space, as in C:\Users\Some Name.
    manager
        .write(
            &info.id,
            &format!("\"{}\" -n 30 127.0.0.1\r", cli.display()),
        )
        .expect("type the command");
    sink.wait_for_state(&info.id, Some(HarnessId::Claude), Some(AgentStatus::Idle));

    // Ctrl+C stops the stand-in, and cmd is in the foreground again.
    manager.write(&info.id, "\u{3}").expect("interrupt");
    sink.wait_for_state(&info.id, None, None);

    manager.close(&info.id).expect("close");
}
