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
    let folder = dir.to_owned();
    std::thread::spawn(move || {
        let _ = serve(
            &folder,
            &Options {
                idle: Duration::from_secs(30),
                prepare: crate::app_image_environment::clean_terminal,
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
