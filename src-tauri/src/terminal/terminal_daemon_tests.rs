//! Sessions run in a real PTY daemon, started in this process on a temporary folder, and the
//! manager reaches it the way it does in the app. Unix-only, like the shell tests.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use lynx_pty_daemon::server::{Options, serve};

use crate::agents::hook_hub::HookHub;
use crate::browser::SessionOwners;

use super::Shell;
use super::daemon_host::{DaemonHost, DaemonState};
use super::terminal_events::TerminalEventSink;
use super::terminal_manager::TerminalManager;
use super::terminal_manager_tests::{is_running, read_pid, wait_until_stopped};
use super::terminal_test_support::{RecordingSink, WAIT, path_of};
use crate::persistence::AppDatabase;

/// A daemon serving `dir` on its own thread, as the app's daemon would.
fn start_daemon(dir: &Path) {
    start_daemon_of(dir, env!("CARGO_PKG_VERSION"));
}

/// A daemon of the given version serving `dir` on its own thread.
fn start_daemon_of(dir: &Path, version: &'static str) {
    let folder = dir.to_owned();
    std::thread::spawn(move || {
        let _ = serve(
            &folder,
            &Options {
                idle: Duration::from_secs(30),
                prepare: crate::process::app_image_environment::clean_terminal,
                version,
            },
        );
    });
    let deadline = std::time::Instant::now() + WAIT;
    while !dir.join("address").exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "the daemon never came up"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn manager_with(daemon: Option<DaemonHost>) -> (TerminalManager, Arc<RecordingSink>) {
    let sink = Arc::new(RecordingSink::default());
    let shell = Arc::new(|| Shell {
        program: "/bin/sh".to_owned(),
        args: Vec::new(),
    });
    let manager = TerminalManager::new(
        shell,
        Arc::clone(&sink) as Arc<dyn TerminalEventSink>,
        HookHub::disabled(),
        SessionOwners::default(),
        daemon,
        None,
    );
    (manager, sink)
}

fn host(dir: PathBuf, enabled: bool) -> DaemonHost {
    DaemonHost::new(dir, None, Arc::new(move || enabled))
}

#[test]
fn a_session_runs_in_the_daemon_and_its_output_comes_back() {
    let daemon_dir = tempfile::tempdir().expect("temp dir");
    start_daemon(daemon_dir.path());
    let (manager, sink) = manager_with(Some(host(daemon_dir.path().to_owned(), true)));
    let work = tempfile::tempdir().expect("temp dir");

    let info = manager
        .create(&path_of(&work), Some("echo daemon-$((20+22))"), 80, 24)
        .expect("create");

    sink.wait_for_text(&info.id, "daemon-42");
    assert!(
        manager.daemon_status().sessions == 1,
        "the session is held by the daemon"
    );
    assert_eq!(manager.daemon_status().state, DaemonState::Connected);
    manager.close(&info.id).expect("close");
}

#[test]
fn input_and_exit_reach_the_daemon_session() {
    let daemon_dir = tempfile::tempdir().expect("temp dir");
    start_daemon(daemon_dir.path());
    let (manager, sink) = manager_with(Some(host(daemon_dir.path().to_owned(), true)));
    let work = tempfile::tempdir().expect("temp dir");
    let info = manager
        .create(&path_of(&work), None, 80, 24)
        .expect("create");

    manager
        .write(&info.id, "echo typed-$((1+1))\r")
        .expect("write");
    sink.wait_for_text(&info.id, "typed-2");
    manager.write(&info.id, "exit 7\r").expect("write");

    assert_eq!(sink.wait_for_exit(&info.id), Some(7));
}

#[test]
fn closing_a_daemon_session_kills_its_program() {
    let daemon_dir = tempfile::tempdir().expect("temp dir");
    start_daemon(daemon_dir.path());
    let (manager, _sink) = manager_with(Some(host(daemon_dir.path().to_owned(), true)));
    let work = tempfile::tempdir().expect("temp dir");
    let pid_file = work.path().join("pid");
    let command = format!("sh -c 'echo $$ > {}; exec sleep 300'", pid_file.display());
    let info = manager
        .create(&path_of(&work), Some(&command), 80, 24)
        .expect("create");
    let pid = read_pid(&pid_file);
    assert!(is_running(&pid));

    manager.close(&info.id).expect("close");

    wait_until_stopped(&pid, Duration::from_secs(8));
}

#[test]
fn with_the_daemon_off_the_session_runs_in_the_app() {
    let daemon_dir = tempfile::tempdir().expect("temp dir");
    start_daemon(daemon_dir.path());
    let (manager, sink) = manager_with(Some(host(daemon_dir.path().to_owned(), false)));
    let work = tempfile::tempdir().expect("temp dir");

    let info = manager
        .create(&path_of(&work), Some("echo local-$((2*3))"), 80, 24)
        .expect("create");

    sink.wait_for_text(&info.id, "local-6");
    assert_eq!(manager.daemon_status().sessions, 0);
    assert!(!manager.daemon_status().enabled);
    manager.close(&info.id).expect("close");
}

#[test]
fn without_a_daemon_to_reach_the_session_falls_back_to_the_app() {
    // Nothing listens here, and there is no program to start one with.
    let nowhere = tempfile::tempdir().expect("temp dir");
    let (manager, sink) = manager_with(Some(host(nowhere.path().to_owned(), true)));
    let work = tempfile::tempdir().expect("temp dir");

    let info = manager
        .create(&path_of(&work), Some("echo fallback-$((4+4))"), 80, 24)
        .expect("create");

    sink.wait_for_text(&info.id, "fallback-8");
    assert_eq!(manager.daemon_status().state, DaemonState::Degraded);
    assert_eq!(manager.daemon_status().sessions, 0);
    manager.close(&info.id).expect("close");
}

fn database() -> (tempfile::TempDir, Arc<AppDatabase>) {
    let dir = tempfile::tempdir().expect("temp dir");
    let database = Arc::new(AppDatabase::open(&dir.path().join("app.db")).expect("database"));
    (dir, database)
}

fn manager_on(
    daemon_dir: &Path,
    database: &Arc<AppDatabase>,
) -> (TerminalManager, Arc<RecordingSink>) {
    let sink = Arc::new(RecordingSink::default());
    let shell = Arc::new(|| Shell {
        program: "/bin/sh".to_owned(),
        args: Vec::new(),
    });
    let manager = TerminalManager::new(
        shell,
        Arc::clone(&sink) as Arc<dyn TerminalEventSink>,
        HookHub::disabled(),
        SessionOwners::default(),
        Some(host(daemon_dir.to_owned(), true)),
        Some(Arc::clone(database)),
    );
    (manager, sink)
}

/// The app closes and opens again: the session the daemon still runs is joined, with its output.
#[test]
fn a_session_the_daemon_still_runs_is_joined_again_after_a_restart() {
    let daemon_dir = tempfile::tempdir().expect("temp dir");
    start_daemon(daemon_dir.path());
    let (_db_dir, database) = database();
    let work = tempfile::tempdir().expect("temp dir");
    let (before, sink) = manager_on(daemon_dir.path(), &database);
    let info = before
        .create(&path_of(&work), Some("echo survived-$((6*7))"), 80, 24)
        .expect("create");
    sink.wait_for_text(&info.id, "survived-42");

    // The first manager goes away without ending anything, as when the app quits and the daemon keeps it.
    before.set_keep_on_exit(true);
    before.quit();
    let (after, later_sink) = manager_on(daemon_dir.path(), &database);
    after.restore();

    assert!(
        after
            .list()
            .iter()
            .any(|session| session.id == info.id && session.running),
        "the restarted app shows the session as running again"
    );
    later_sink.wait_for_text(&info.id, "survived-42");
    after.close(&info.id).expect("close");
}

/// A restart keeps the program running, not just its record: the same process is still there after
/// the old app quits, and the new one joins it.
#[test]
fn a_restart_keeps_the_daemon_process_running() {
    let daemon_dir = tempfile::tempdir().expect("temp dir");
    start_daemon(daemon_dir.path());
    let (_db_dir, database) = database();
    let work = tempfile::tempdir().expect("temp dir");
    let pid_file = work.path().join("pid");
    let command = format!("sh -c 'echo $$ > {}; exec sleep 300'", pid_file.display());
    let (before, _sink) = manager_on(daemon_dir.path(), &database);
    let info = before
        .create(&path_of(&work), Some(&command), 80, 24)
        .expect("create");
    let pid = read_pid(&pid_file);

    before.set_keep_on_exit(true);
    before.quit();

    assert!(is_running(&pid), "the restart must not end the program");
    let (after, _later) = manager_on(daemon_dir.path(), &database);
    after.restore();
    assert!(
        after.list().iter().any(|session| session.id == info.id),
        "the new app rejoins the session"
    );
    after.close(&info.id).expect("close");
}

/// The window opens again at another size: the rejoined session must follow it.
#[test]
fn a_rejoined_session_follows_the_size_the_window_gives_it() {
    let daemon_dir = tempfile::tempdir().expect("temp dir");
    start_daemon(daemon_dir.path());
    let (_db_dir, database) = database();
    let work = tempfile::tempdir().expect("temp dir");
    let (before, sink) = manager_on(daemon_dir.path(), &database);
    let info = before
        .create(&path_of(&work), Some("echo ready-$((6*7))"), 100, 30)
        .expect("create");
    sink.wait_for_text(&info.id, "ready-42");
    before.set_keep_on_exit(true);
    before.quit();
    let (after, later_sink) = manager_on(daemon_dir.path(), &database);
    after.restore();

    after.resize(&info.id, 150, 45).expect("resize");
    after
        .write(&info.id, "echo size-$(stty size)\r")
        .expect("write");

    later_sink.wait_for_text(&info.id, "size-45 150");
    after.close(&info.id).expect("close");
}

/// A session with no record in the app is the daemon's leftover, and it is ended.
#[test]
fn a_daemon_session_the_app_has_no_record_of_is_ended_when_the_app_starts() {
    let daemon_dir = tempfile::tempdir().expect("temp dir");
    start_daemon(daemon_dir.path());
    let (_db_dir, database) = database();
    let (stranger, sink) = manager_with(Some(host(daemon_dir.path().to_owned(), true)));
    let work = tempfile::tempdir().expect("temp dir");
    let info = stranger
        .create(&path_of(&work), Some("sleep 300"), 80, 24)
        .expect("create");

    let (after, _) = manager_on(daemon_dir.path(), &database);
    after.restore();

    sink.wait_for_exit(&info.id);
}

#[test]
fn a_manager_without_a_daemon_reports_it_off() {
    let (manager, _sink) = manager_with(None);
    let status = manager.daemon_status();
    assert!(!status.enabled);
    assert_eq!(status.state, DaemonState::Idle);
}

#[test]
fn resizing_a_daemon_session_changes_the_size_its_program_sees() {
    let daemon_dir = tempfile::tempdir().expect("temp dir");
    start_daemon(daemon_dir.path());
    let (manager, sink) = manager_with(Some(host(daemon_dir.path().to_owned(), true)));
    let work = tempfile::tempdir().expect("temp dir");
    let info = manager
        .create(&path_of(&work), None, 80, 24)
        .expect("create");

    manager.resize(&info.id, 132, 40).expect("resize");
    manager
        .write(&info.id, "echo size-$(stty size)\r")
        .expect("write");

    sink.wait_for_text(&info.id, "size-40 132");
    manager.close(&info.id).expect("close");
}

#[test]
fn the_daemon_reports_the_memory_its_sessions_hold() {
    let daemon_dir = tempfile::tempdir().expect("temp dir");
    start_daemon(daemon_dir.path());
    let (manager, _sink) = manager_with(Some(host(daemon_dir.path().to_owned(), true)));
    let work = tempfile::tempdir().expect("temp dir");
    let info = manager
        .create(&path_of(&work), Some("sleep 300"), 80, 24)
        .expect("create");

    assert!(manager.daemon_memory_mib().is_some());
    manager.close(&info.id).expect("close");
}

#[test]
fn without_a_daemon_there_is_no_memory_to_report() {
    let (manager, _sink) = manager_with(None);
    assert_eq!(manager.daemon_memory_mib(), None);
}

/// A session a command line starts while the app is closed is shown when the app opens, with its
/// output, and the daemon stops listing it.
#[test]
fn a_session_started_from_the_command_line_is_shown_when_the_app_opens() {
    let daemon_dir = tempfile::tempdir().expect("temp dir");
    start_daemon(daemon_dir.path());
    let (_db_dir, database) = database();
    let work = tempfile::tempdir().expect("temp dir");
    let daemon = host(daemon_dir.path().to_owned(), true);
    let id = daemon
        .start_outside(Path::new(&path_of(&work)), Some("echo cli-$((6*7))"))
        .expect("start from the command line");

    let (after, sink) = manager_on(daemon_dir.path(), &database);
    after.restore();

    sink.wait_for_text(&id, "cli-42");
    assert!(
        after
            .list()
            .iter()
            .any(|session| session.id == id && session.running),
        "the app shows the session the command line started"
    );
    assert_eq!(daemon.outside_sessions().expect("answer"), Vec::new());
    after.close(&id).expect("close");
}

/// An agent event that arrives while the app is closed is kept by the daemon, and the session's
/// agent state gets it when the app opens.
#[test]
fn an_event_left_while_the_app_is_closed_reaches_the_session_when_it_opens() {
    let daemon_dir = tempfile::tempdir().expect("temp dir");
    start_daemon(daemon_dir.path());
    let (_db_dir, database) = database();
    let work = tempfile::tempdir().expect("temp dir");
    let (before, _sink) = manager_on(daemon_dir.path(), &database);
    let info = before
        .create(&path_of(&work), None, 80, 24)
        .expect("create");
    before.set_keep_on_exit(true);
    before.quit();

    let delivered = crate::agents::hook_relay::deliver_to_daemon(
        daemon_dir.path(),
        &info.id,
        "claude",
        &serde_json::json!({
            "hook_event_name": "SessionStart",
            "source": "startup",
            "session_id": "conversation-9",
        }),
    );
    assert!(
        delivered,
        "the daemon keeps the event while the app is closed"
    );

    let (after, _later) = manager_on(daemon_dir.path(), &database);
    after.restore();

    let deadline = std::time::Instant::now() + WAIT;
    while database
        .terminal_session(&info.id)
        .expect("read")
        .and_then(|record| record.resume_id)
        .as_deref()
        != Some("conversation-9")
    {
        assert!(
            std::time::Instant::now() < deadline,
            "the stored event never reached the session"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    let daemon = host(daemon_dir.path().to_owned(), true);
    assert!(
        daemon.take_events(&info.id).is_empty(),
        "the events were taken by the app"
    );
    after.close(&info.id).expect("close");
}

/// With no daemon in the folder, an agent event has nowhere to wait, and the hook is told so.
#[test]
fn without_a_daemon_an_agent_event_has_nowhere_to_wait() {
    let folder = tempfile::tempdir().expect("temp dir");
    assert!(!crate::agents::hook_relay::deliver_to_daemon(
        folder.path(),
        "session",
        "claude",
        &serde_json::json!({ "hook_event_name": "Stop" }),
    ));
}

/// Waits up to two seconds for `ps` to show the process frozen (`T`), or running when `frozen` is
/// false. Returns what `ps` last showed.
fn frozen_within_two_seconds(pid: u32, frozen: bool) -> String {
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    loop {
        let shown = std::process::Command::new("ps")
            .args(["-o", "stat=", "-p", &pid.to_string()])
            .output()
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
            .unwrap_or_default();
        if shown.starts_with('T') == frozen || std::time::Instant::now() >= deadline {
            return shown;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A terminal the daemon runs can sleep, and typing into it wakes it with the input.
#[test]
fn a_daemon_terminal_sleeps_and_wakes_when_typed_into() {
    let daemon_dir = tempfile::tempdir().expect("temp dir");
    start_daemon(daemon_dir.path());
    let (manager, sink) = manager_with(Some(host(daemon_dir.path().to_owned(), true)));
    let work = tempfile::tempdir().expect("temp dir");
    let pid_file = work.path().join("pid");
    let command = format!(
        "sh -c 'echo $$ > {}; while read line; do echo got-$line; done'",
        pid_file.display()
    );
    let info = manager
        .create(&path_of(&work), Some(&command), 80, 24)
        .expect("create");
    let pid: u32 = read_pid(&pid_file).trim().parse().expect("a process id");

    manager.hibernate(&info.id).expect("sleep");

    assert!(
        manager
            .list()
            .iter()
            .any(|session| session.id == info.id && session.hibernated)
    );
    let state = frozen_within_two_seconds(pid, true);
    assert!(
        state.starts_with('T'),
        "the program is frozen, ps shows {state:?}"
    );
    manager.write(&info.id, "hello\r").expect("type");
    sink.wait_for_text(&info.id, "got-hello");
    assert!(
        manager
            .list()
            .iter()
            .any(|session| session.id == info.id && !session.hibernated),
        "typing woke the terminal"
    );
    let state = frozen_within_two_seconds(pid, false);
    assert!(
        !state.starts_with('T'),
        "the program runs again, ps shows {state:?}"
    );
    manager.close(&info.id).expect("close");
}

/// A terminal the app runs itself has nothing to freeze it with, so it is refused, with the reason.
#[test]
fn a_terminal_outside_the_daemon_cannot_sleep() {
    let (manager, _sink) = manager_with(None);
    let work = tempfile::tempdir().expect("temp dir");
    let info = manager
        .create(&path_of(&work), None, 80, 24)
        .expect("create");

    assert!(matches!(
        manager.hibernate(&info.id),
        Err(super::TerminalError::Refused(_))
    ));
    manager.close(&info.id).expect("close");
}

/// A daemon left by an older version, with nothing running in it, is replaced when the app opens.
#[test]
fn a_daemon_of_another_version_with_no_sessions_is_replaced_when_the_app_opens() {
    let daemon_dir = tempfile::tempdir().expect("temp dir");
    start_daemon_of(daemon_dir.path(), "0.0.0-older");
    let (_db_dir, database) = database();

    let (after, _sink) = manager_on(daemon_dir.path(), &database);
    after.restore();

    assert!(
        !lynx_pty_daemon::server::is_running(daemon_dir.path()),
        "the older daemon was left running"
    );
}

/// A daemon of this version is left alone when the app opens.
#[test]
fn a_daemon_of_this_version_is_kept_when_the_app_opens() {
    let daemon_dir = tempfile::tempdir().expect("temp dir");
    start_daemon(daemon_dir.path());
    let (_db_dir, database) = database();

    let (after, _sink) = manager_on(daemon_dir.path(), &database);
    after.restore();

    assert!(lynx_pty_daemon::server::is_running(daemon_dir.path()));
}
