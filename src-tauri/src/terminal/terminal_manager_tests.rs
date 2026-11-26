//! Runs real shells on a PTY, so these are Unix-only.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
#[cfg(not(target_os = "linux"))]
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::agents::activity::AgentStatus;
use crate::agents::harness::{HarnessId, harness_name};
use crate::agents::hook_hub::{HookHub, PORT_ENV, TOKEN_ENV};
use crate::browser::{PageOwner, SessionOwners};
use crate::config::app_config::LaunchMode;
use crate::persistence::{AppDatabase, TerminalSessionRecord};

use super::terminal_events::TerminalEventSink;
use super::terminal_manager::{TerminalManager, relaunch_line};
use super::terminal_test_support::{RecordingSink, WAIT, output_of, path_of};
use super::{Shell, TerminalError};

fn setup() -> (TerminalManager, Arc<RecordingSink>, tempfile::TempDir) {
    let sink = Arc::new(RecordingSink::default());
    // A bare `sh` keeps prompts and rc files out of the way.
    let shell = Arc::new(|| Shell {
        program: "/bin/sh".to_owned(),
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
    (manager, sink, tempfile::tempdir().expect("temp dir"))
}

pub(super) fn is_running(pid: &str) -> bool {
    #[cfg(target_os = "linux")]
    {
        // A zombie has already exited; it only waits to be reaped.
        match fs::read_to_string(format!("/proc/{pid}/stat")) {
            Ok(stat) => !stat
                .rsplit(") ")
                .next()
                .is_some_and(|rest| rest.starts_with('Z')),
            Err(_) => false,
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        Command::new("kill")
            .args(["-0", pid])
            .status()
            .is_ok_and(|status| status.success())
    }
}

pub(super) fn read_pid(file: &Path) -> String {
    let deadline = Instant::now() + WAIT;
    loop {
        if let Ok(pid) = fs::read_to_string(file)
            && !pid.trim().is_empty()
        {
            return pid.trim().to_owned();
        }
        assert!(Instant::now() < deadline, "the program never wrote its pid");
        std::thread::sleep(Duration::from_millis(20));
    }
}

pub(super) fn wait_until_stopped(pid: &str, within: Duration) {
    let deadline = Instant::now() + within;
    while is_running(pid) {
        assert!(Instant::now() < deadline, "process {pid} is still running");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn a_command_is_typed_into_the_shell_and_its_output_is_streamed() {
    let (manager, sink, dir) = setup();

    let info = manager
        .create(&path_of(&dir), Some("echo agent-$((20+22))"), 80, 24)
        .expect("create");

    // The typed command echoes `$((20+22))`; only the result contains `agent-42`.
    sink.wait_for_text(&info.id, "agent-42");
    assert!(info.running);
    assert_eq!(info.command.as_deref(), Some("echo agent-$((20+22))"));
    assert_eq!(info.shell, "sh");
}

#[test]
fn the_shell_starts_in_the_requested_directory() {
    let (manager, sink, dir) = setup();
    let cwd = path_of(&dir);

    let info = manager.create(&cwd, Some("pwd"), 80, 24).expect("create");

    sink.wait_for_text(&info.id, &format!("{cwd}\r\n"));
}

#[test]
fn the_snapshot_carries_the_size_the_session_runs_at_and_a_resize_changes_it() {
    let (manager, _sink, dir) = setup();
    let info = manager
        .create(&path_of(&dir), None, 100, 30)
        .expect("create");
    let snapshot = manager.snapshot(&info.id).expect("snapshot");
    assert_eq!((snapshot.cols, snapshot.rows), (100, 30));

    manager.resize(&info.id, 120, 40).expect("resize");
    let snapshot = manager.snapshot(&info.id).expect("snapshot");
    assert_eq!((snapshot.cols, snapshot.rows), (120, 40));
}

#[test]
fn input_reaches_the_running_shell() {
    let (manager, sink, dir) = setup();
    let info = manager
        .create(&path_of(&dir), None, 80, 24)
        .expect("create");

    manager
        .write(&info.id, "echo typed-$((1+1))\r")
        .expect("write");

    sink.wait_for_text(&info.id, "typed-2");
}

#[test]
fn resizing_changes_the_size_the_program_sees() {
    let (manager, sink, dir) = setup();
    let info = manager
        .create(&path_of(&dir), Some("stty size"), 100, 30)
        .expect("create");
    sink.wait_for_text(&info.id, "30 100");

    manager.resize(&info.id, 120, 40).expect("resize");
    manager.write(&info.id, "stty size\r").expect("write");

    sink.wait_for_text(&info.id, "40 120");
}

#[test]
fn exit_reports_the_status_and_the_session_stays_readable() {
    let (manager, sink, dir) = setup();

    let info = manager
        .create(&path_of(&dir), Some("echo last-words; exit 7"), 80, 24)
        .expect("create");

    assert_eq!(sink.wait_for_exit(&info.id), Some(7));

    let listed = manager.list();
    assert_eq!(listed.len(), 1);
    assert!(!listed[0].running);
    assert_eq!(listed[0].exit_code, Some(7));
    assert!(
        manager
            .snapshot(&info.id)
            .expect("snapshot")
            .data
            .contains("last-words")
    );
    assert!(matches!(
        manager.write(&info.id, "x"),
        Err(TerminalError::NotRunning)
    ));
}

#[test]
fn the_exit_is_announced_after_all_output_and_the_snapshot_matches_the_events() {
    let (manager, sink, dir) = setup();
    let info = manager
        .create(&path_of(&dir), Some("echo one; echo two; exit 0"), 80, 24)
        .expect("create");

    sink.wait_for_exit(&info.id);

    let snapshot = manager.snapshot(&info.id).expect("snapshot");
    let events = sink.events.lock().expect("lock");
    let seqs: Vec<u32> = events.output.iter().map(|(_, seq, _)| *seq).collect();
    assert_eq!(
        seqs,
        (1..=seqs.len() as u32).collect::<Vec<_>>(),
        "seq counts chunks from 1"
    );
    assert_eq!(snapshot.last_seq, *seqs.last().expect("some output"));
    assert_eq!(snapshot.data, output_of(&events, &info.id));
    assert!(snapshot.data.contains("one") && snapshot.data.contains("two"));
}

#[test]
fn closing_kills_the_foreground_program_not_just_the_shell() {
    let (manager, sink, dir) = setup();
    let pid_file = dir.path().join("pid");
    let command = format!("sh -c 'echo $$ > {}; exec sleep 300'", pid_file.display());
    let info = manager
        .create(&path_of(&dir), Some(&command), 80, 24)
        .expect("create");
    let pid = read_pid(&pid_file);
    assert!(is_running(&pid));

    manager.close(&info.id).expect("close");

    wait_until_stopped(&pid, Duration::from_secs(8));
    assert!(manager.list().is_empty());
    drop(sink);
}

#[test]
fn a_program_that_ignores_hangup_is_killed_after_the_grace_period() {
    let (manager, _sink, dir) = setup();
    let pid_file = dir.path().join("pid");
    let command = format!(
        "sh -c 'trap \"\" HUP; echo $$ > {}; exec sleep 300'",
        pid_file.display()
    );
    let info = manager
        .create(&path_of(&dir), Some(&command), 80, 24)
        .expect("create");
    let pid = read_pid(&pid_file);

    manager.close(&info.id).expect("close");

    wait_until_stopped(&pid, Duration::from_secs(10));
}

#[test]
fn a_session_the_backend_closes_leaves_the_list_and_the_screen_is_told() {
    let (manager, sink, dir) = setup();
    let info = manager
        .create(&path_of(&dir), Some("sleep 300"), 80, 24)
        .expect("create");

    manager.close_and_announce(&info.id).expect("close");

    assert!(manager.list().is_empty(), "closed, not just ended");
    assert_eq!(
        sink.events.lock().expect("lock").closed,
        vec![info.id.clone()]
    );
}

#[test]
fn a_shell_the_person_closes_is_not_announced_as_closed_by_the_backend() {
    let (manager, sink, dir) = setup();
    let info = manager
        .create(&path_of(&dir), None, 80, 24)
        .expect("create");

    manager.close(&info.id).expect("close");

    assert!(manager.list().is_empty());
    assert!(sink.events.lock().expect("lock").closed.is_empty());
}

#[test]
fn close_all_stops_every_session() {
    let (manager, _sink, dir) = setup();
    let cwd = path_of(&dir);
    let pid_files: Vec<_> = (0..2).map(|n| dir.path().join(format!("pid{n}"))).collect();
    for file in &pid_files {
        let command = format!("sh -c 'echo $$ > {}; exec sleep 300'", file.display());
        manager
            .create(&cwd, Some(&command), 80, 24)
            .expect("create");
    }
    let pids: Vec<_> = pid_files.iter().map(|file| read_pid(file)).collect();

    manager.close_all();

    assert!(manager.list().is_empty());
    for pid in &pids {
        wait_until_stopped(pid, Duration::from_secs(8));
    }
}

#[test]
fn bad_requests_are_rejected_without_creating_a_session() {
    let (manager, _sink, dir) = setup();
    let cwd = path_of(&dir);

    assert!(matches!(
        manager.create("/definitely/not/here", None, 80, 24),
        Err(TerminalError::InvalidWorkingDirectory(_))
    ));
    assert!(matches!(
        manager.create(&cwd, None, 0, 24),
        Err(TerminalError::InvalidSize)
    ));
    assert!(matches!(
        manager.create(&cwd, None, 80, 0),
        Err(TerminalError::InvalidSize)
    ));
    assert!(matches!(
        manager.snapshot("nope"),
        Err(TerminalError::NotFound(_))
    ));
    assert!(matches!(
        manager.write("nope", "x"),
        Err(TerminalError::NotFound(_))
    ));
    assert!(matches!(
        manager.resize("nope", 80, 24),
        Err(TerminalError::NotFound(_))
    ));
    assert!(matches!(
        manager.close("nope"),
        Err(TerminalError::NotFound(_))
    ));
    assert!(manager.list().is_empty());
}

#[test]
fn a_blank_command_just_opens_a_shell() {
    let (manager, sink, dir) = setup();

    let info = manager
        .create(&path_of(&dir), Some("   "), 80, 24)
        .expect("create");
    manager
        .write(&info.id, "echo alive-$((2*3))\r")
        .expect("write");

    assert_eq!(info.command, None);
    sink.wait_for_text(&info.id, "alive-6");
}

#[test]
fn closing_a_session_that_already_exited_signals_nothing_and_forgets_it() {
    let (manager, sink, dir) = setup();
    let info = manager
        .create(&path_of(&dir), Some("exit 0"), 80, 24)
        .expect("create");
    sink.wait_for_exit(&info.id);

    manager.close(&info.id).expect("close");

    assert!(manager.list().is_empty());
}

#[test]
fn a_session_records_when_it_started_and_keeps_that_time() {
    let (manager, _sink, dir) = setup();
    let before = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs() as u32;

    let info = manager
        .create(&path_of(&dir), None, 80, 24)
        .expect("create");

    let after = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs() as u32;
    assert!(info.started_at >= before && info.started_at <= after);
    assert_eq!(manager.list()[0].started_at, info.started_at);
}

#[test]
fn a_session_belongs_to_its_worktree_until_it_is_closed() {
    let owners = SessionOwners::default();
    let shell = Arc::new(|| Shell {
        program: "/bin/sh".to_owned(),
        args: Vec::new(),
    });
    let manager = TerminalManager::new(
        shell,
        Arc::new(RecordingSink::default()) as Arc<dyn TerminalEventSink>,
        HookHub::disabled(),
        owners.clone(),
        None,
        None,
    );
    let dir = tempfile::tempdir().expect("temp dir");
    let info = manager
        .create(&path_of(&dir), None, 80, 24)
        .expect("create");
    assert_eq!(
        owners.get(&info.id),
        Some(PageOwner::Workspace {
            worktree_path: path_of(&dir),
        })
    );

    manager.close(&info.id).expect("close");

    assert_eq!(owners.get(&info.id), None);
}

/// Posts a hook report the way the relay does, to the listener the session was given.
fn post_hook(hub: &HookHub, session: &str, harness: &str, payload: &str) -> String {
    let env = hub.session_env(session);
    let value = |key: &str| {
        env.iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.clone())
            .expect("the session has hook variables")
    };
    let port: u16 = value(PORT_ENV).parse().expect("port");
    let body = format!(r#"{{"sessionId":"{session}","harness":"{harness}","payload":{payload}}}"#);
    let request = format!(
        "POST /agent-hook HTTP/1.1\r\nHost: 127.0.0.1\r\nX-Lynx-Token: {}\r\nContent-Length: {}\r\n\r\n{body}",
        value(TOKEN_ENV),
        body.len()
    );
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect to the listener");
    stream.write_all(request.as_bytes()).expect("write");
    let mut answer = String::new();
    stream.read_to_string(&mut answer).expect("read");
    answer.lines().next().unwrap_or_default().to_owned()
}

/// A shell turns into the harness that reports from it, and back when that harness ends.
fn a_shell_becomes_the_harness_that_reports_from_it_and_back(
    harness: HarnessId,
    start: &str,
    end: &str,
) {
    let sink = Arc::new(RecordingSink::default());
    let hub = HookHub::start();
    let shell = Arc::new(|| Shell {
        program: "/bin/sh".to_owned(),
        args: Vec::new(),
    });
    let manager = TerminalManager::new(
        shell,
        Arc::clone(&sink) as Arc<dyn TerminalEventSink>,
        hub.clone(),
        SessionOwners::default(),
        None,
        None,
    );
    let dir = tempfile::tempdir().expect("temp dir");
    let info = manager
        .create(&path_of(&dir), None, 80, 24)
        .expect("create");
    assert_eq!(info.harness, None, "a new shell is not an agent");
    let name = harness_name(harness);

    let reply = post_hook(&hub, &info.id, name, start);
    assert_eq!(reply, "HTTP/1.1 204 No Content");
    sink.wait_for_state(&info.id, Some(harness), Some(AgentStatus::Idle));
    let listed = manager
        .list()
        .into_iter()
        .find(|session| session.id == info.id)
        .expect("listed");
    assert_eq!(listed.harness, Some(harness));

    post_hook(&hub, &info.id, name, end);
    sink.wait_for_state(&info.id, None, None);
    let listed = manager
        .list()
        .into_iter()
        .find(|session| session.id == info.id)
        .expect("listed");
    assert_eq!(listed.harness, None, "the session is a shell again");

    manager.close(&info.id).expect("close");
}

/// A stand-in for an agent CLI: a script under the name the agent goes by, which waits the way an
/// agent does until it is interrupted.
fn fake_cli(dir: &Path, name: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let bin = dir.join("bin");
    fs::create_dir_all(&bin).expect("bin dir");
    let script = bin.join(name);
    fs::write(&script, "#!/bin/sh\nsleep 30\n").expect("write");
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).expect("chmod");
    bin
}

#[test]
fn an_agent_typed_by_hand_in_a_shell_is_followed_until_it_exits() {
    let sink = Arc::new(RecordingSink::default());
    let hub = HookHub::start();
    let shell = Arc::new(|| Shell {
        program: "/bin/sh".to_owned(),
        args: Vec::new(),
    });
    let manager = TerminalManager::new(
        shell,
        Arc::clone(&sink) as Arc<dyn TerminalEventSink>,
        hub.clone(),
        SessionOwners::default(),
        None,
        None,
    );
    let dir = tempfile::tempdir().expect("temp dir");
    let bin = fake_cli(dir.path(), "claude");
    let info = manager
        .create(&path_of(&dir), None, 80, 24)
        .expect("create");
    assert_eq!(info.harness, None, "a new shell is not an agent");

    manager
        .write(&info.id, &format!("PATH={}:$PATH claude\r", bin.display()))
        .expect("type the command");
    sink.wait_for_state(&info.id, Some(HarnessId::Claude), Some(AgentStatus::Idle));

    // Ctrl+C stops the stand-in, and the shell is in the foreground again.
    manager.write(&info.id, "\u{3}").expect("interrupt");
    sink.wait_for_state(&info.id, None, None);

    manager.close(&info.id).expect("close");
}

#[test]
fn a_shell_that_runs_claude_becomes_an_agent_and_back_when_it_ends() {
    a_shell_becomes_the_harness_that_reports_from_it_and_back(
        HarnessId::Claude,
        r#"{"hook_event_name":"SessionStart","source":"startup"}"#,
        r#"{"hook_event_name":"SessionEnd","reason":"other"}"#,
    );
}

#[test]
fn a_shell_that_runs_codex_becomes_an_agent_and_back_when_it_ends() {
    a_shell_becomes_the_harness_that_reports_from_it_and_back(
        HarnessId::Codex,
        r#"{"hook_event_name":"SessionStart"}"#,
        r#"{"hook_event_name":"SessionEnd"}"#,
    );
}

#[test]
fn a_shell_that_runs_opencode_becomes_an_agent_and_back_when_it_ends() {
    a_shell_becomes_the_harness_that_reports_from_it_and_back(
        HarnessId::Opencode,
        r#"{"type":"session.created"}"#,
        r#"{"type":"session.deleted"}"#,
    );
}

/// One run of the app over a database, built the way the app builds its manager.
fn app_run(database: &Arc<AppDatabase>, sink: &Arc<RecordingSink>) -> TerminalManager {
    TerminalManager::new(
        Arc::new(|| Shell {
            program: "/bin/sh".to_owned(),
            args: Vec::new(),
        }),
        Arc::clone(sink) as Arc<dyn TerminalEventSink>,
        HookHub::disabled(),
        SessionOwners::default(),
        None,
        Some(Arc::clone(database)),
    )
}

fn database_in(dir: &tempfile::TempDir) -> Arc<AppDatabase> {
    Arc::new(AppDatabase::open(&dir.path().join("app.db")).expect("database"))
}

/// A session as the app leaves it at quit: ended, and open at quit.
fn left_at_quit(
    id: &str,
    worktree: &str,
    harness: Option<&str>,
    resume_id: Option<&str>,
    shell: &str,
) -> TerminalSessionRecord {
    TerminalSessionRecord {
        id: id.to_owned(),
        worktree_path: worktree.to_owned(),
        harness: harness.map(str::to_owned),
        command: None,
        shell: shell.to_owned(),
        resume_id: resume_id.map(str::to_owned),
        started_at: 10,
        ended_at: Some(20),
        exit_code: None,
        open_at_quit: true,
        agent_title: None,
    }
}

/// Closing the app leaves the agent sessions it had open waiting to be restored, with their
/// conversation. A session the person closed before that is not among them.
#[test]
fn the_sessions_open_at_quit_wait_to_be_restored_and_a_closed_one_does_not() {
    let dir = tempfile::tempdir().expect("temp dir");
    let database = database_in(&dir);
    let sink = Arc::new(RecordingSink::default());
    let manager = app_run(&database, &sink);
    let open = manager
        .create(&path_of(&dir), None, 80, 24)
        .expect("create");
    database
        .set_terminal_harness(&open.id, "claude")
        .expect("harness");
    database
        .set_terminal_resume_id(&open.id, "conv-open")
        .expect("resume id");
    let closed = manager
        .create(&path_of(&dir), None, 80, 24)
        .expect("create");
    manager.close(&closed.id).expect("close");
    sink.wait_for_exit(&closed.id);
    manager.quit();
    sink.wait_for_exit(&open.id);

    let next = app_run(&database, &Arc::new(RecordingSink::default()));
    next.restore();
    let listed = next.list();
    assert_eq!(listed.len(), 1, "only the agent open at quit is listed");
    assert_eq!(listed[0].id, open.id);
    assert!(listed[0].restorable && !listed[0].running);
    assert!(next.info(&closed.id).is_err(), "the closed session is gone");
}

/// Only an agent that named its conversation can be restored. Any other session that was open at
/// quit stays in the database, unlisted, with no warning.
#[test]
fn a_session_without_an_agent_or_its_conversation_is_not_listed_after_a_restart() {
    let dir = tempfile::tempdir().expect("temp dir");
    let database = database_in(&dir);
    let worktree = path_of(&dir);
    for record in [
        left_at_quit("plain", &worktree, None, None, "/bin/sh"),
        left_at_quit("unnamed", &worktree, Some("codex"), None, "/bin/sh"),
        left_at_quit(
            "named",
            &worktree,
            Some("claude"),
            Some("conv-1"),
            "/bin/sh",
        ),
    ] {
        database.insert_terminal_session(&record).expect("insert");
    }

    let next = app_run(&database, &Arc::new(RecordingSink::default()));
    next.restore();

    let listed: Vec<String> = next.list().into_iter().map(|info| info.id).collect();
    assert_eq!(listed, ["named"]);
    assert!(next.info("plain").is_err());
    assert!(next.info("unnamed").is_err());
    assert!(database.terminal_session("plain").expect("read").is_some());
    assert!(
        database
            .terminal_session("unnamed")
            .expect("read")
            .is_some()
    );
}

/// A session the app never ended, as after a crash, waits to be restored too, with its conversation.
#[test]
fn a_session_the_app_never_ended_waits_to_be_restored_too() {
    let dir = tempfile::tempdir().expect("temp dir");
    let database = database_in(&dir);
    database
        .insert_terminal_session(&TerminalSessionRecord {
            id: "crashed".to_owned(),
            worktree_path: path_of(&dir),
            harness: Some("claude".to_owned()),
            command: None,
            shell: "/bin/sh".to_owned(),
            resume_id: Some("conv-crashed".to_owned()),
            started_at: 10,
            ended_at: None,
            exit_code: None,
            open_at_quit: false,
            agent_title: Some("Fix the login".to_owned()),
        })
        .expect("insert");

    let next = app_run(&database, &Arc::new(RecordingSink::default()));
    next.restore();

    let listed = next.list();
    assert_eq!(
        listed
            .iter()
            .map(|info| info.id.as_str())
            .collect::<Vec<_>>(),
        ["crashed"]
    );
    assert!(listed[0].restorable && !listed[0].running);
    assert_eq!(listed[0].title.as_deref(), Some("Fix the login"));
    assert!(
        database
            .terminal_session("crashed")
            .expect("read")
            .expect("row")
            .ended_at
            .is_some(),
        "the record says it ended when the app stopped"
    );
}

/// A restored session runs again under the id it had, and is listed once, as running. The restored
/// line is typed into `cat`, which echoes it and runs none of it, so no agent starts in the test.
#[test]
fn a_restored_session_runs_again_under_the_same_id() {
    let dir = tempfile::tempdir().expect("temp dir");
    let database = database_in(&dir);
    database
        .insert_terminal_session(&left_at_quit(
            "agent",
            &path_of(&dir),
            Some("claude"),
            Some("conv-1"),
            "/bin/cat",
        ))
        .expect("insert");

    let sink = Arc::new(RecordingSink::default());
    let next = app_run(&database, &sink);
    next.restore();
    let info = next
        .relaunch("agent", LaunchMode::Manual, &BTreeMap::new())
        .expect("relaunch");

    assert_eq!(info.id, "agent");
    assert!(info.running && !info.restorable);
    assert_eq!(next.list().len(), 1, "listed once, as running");
    let record = database
        .terminal_session("agent")
        .expect("read")
        .expect("row");
    assert_eq!(record.ended_at, None, "open again");
    assert!(!record.open_at_quit);

    next.write("agent", "back-restored\r")
        .expect("the restored session takes input");
    sink.wait_for_text("agent", "back-restored");
}

/// Only a session that waits can be restored, and it has no process until then. Closing one that
/// waits forgets its record.
#[test]
fn only_a_waiting_session_can_be_restored_and_closing_one_forgets_it() {
    let dir = tempfile::tempdir().expect("temp dir");
    let database = database_in(&dir);
    database
        .insert_terminal_session(&left_at_quit(
            "open",
            &path_of(&dir),
            Some("claude"),
            Some("conv-1"),
            "/bin/sh",
        ))
        .expect("insert");

    let next = app_run(&database, &Arc::new(RecordingSink::default()));
    next.restore();
    assert!(matches!(
        next.relaunch("missing", LaunchMode::Manual, &BTreeMap::new()),
        Err(TerminalError::Refused(_))
    ));
    assert!(matches!(
        next.write("open", "x"),
        Err(TerminalError::NotRunning)
    ));

    next.close("open").expect("close");
    assert!(next.list().is_empty());
    assert!(database.terminal_session("open").expect("read").is_none());
    assert!(matches!(
        next.relaunch("open", LaunchMode::Manual, &BTreeMap::new()),
        Err(TerminalError::Refused(_))
    ));
}

/// The name the agent gave the conversation is what a live session lists, so its tab shows it.
#[test]
fn a_live_session_lists_the_name_its_agent_gave_the_conversation() {
    let dir = tempfile::tempdir().expect("temp dir");
    let database = database_in(&dir);
    let manager = app_run(&database, &Arc::new(RecordingSink::default()));
    let info = manager
        .create(&path_of(&dir), None, 80, 24)
        .expect("create");
    assert_eq!(manager.info(&info.id).expect("info").title, None);

    database
        .set_terminal_agent_title(&info.id, "Corrigir o login")
        .expect("title");
    assert_eq!(
        manager.info(&info.id).expect("info").title.as_deref(),
        Some("Corrigir o login")
    );
    let listed = manager.list();
    assert_eq!(listed[0].title.as_deref(), Some("Corrigir o login"));
    manager.close(&info.id).expect("close");
}

#[test]
fn a_restored_agent_starts_again_with_the_arguments_that_resume_it() {
    let none = BTreeMap::new();
    assert_eq!(
        relaunch_line(HarnessId::Claude, LaunchMode::Manual, &none, Some("conv-1")),
        "claude --resume conv-1"
    );
    assert_eq!(
        relaunch_line(HarnessId::Codex, LaunchMode::Manual, &none, Some("conv-1")),
        "codex resume conv-1"
    );
    assert_eq!(
        relaunch_line(HarnessId::Claude, LaunchMode::Manual, &none, None),
        "claude",
        "without a conversation it starts fresh"
    );
}

/// The restored line is the one Settings start the agent with: YOLO and the typed arguments come back
/// with it, and the conversation is named last.
#[test]
fn a_restored_agent_starts_again_in_yolo_with_its_settings_arguments() {
    let set = BTreeMap::from([(HarnessId::Claude, "--model opus".to_owned())]);
    assert_eq!(
        relaunch_line(HarnessId::Claude, LaunchMode::Yolo, &set, Some("conv-1")),
        "claude --dangerously-skip-permissions --model opus --resume conv-1"
    );
    assert_eq!(
        relaunch_line(HarnessId::Claude, LaunchMode::Yolo, &set, None),
        "claude --dangerously-skip-permissions --model opus"
    );
    let codex = BTreeMap::from([(HarnessId::Codex, "--search".to_owned())]);
    assert_eq!(
        relaunch_line(HarnessId::Codex, LaunchMode::Yolo, &codex, Some("conv-1")),
        "codex --dangerously-bypass-approvals-and-sandbox --search resume conv-1"
    );
}

/// Every session the manager starts is announced once, with its id, so the screen can list it
/// whoever asked for it.
#[test]
fn a_started_session_is_announced_once_with_its_id() {
    let (manager, sink, dir) = setup();
    let info = manager
        .create(&path_of(&dir), None, 80, 24)
        .expect("create");
    let rejected = manager.create("/no/such/folder", None, 80, 24);
    assert!(rejected.is_err());

    assert_eq!(
        sink.events.lock().expect("lock").created,
        vec![info.id.clone()]
    );
    manager.close(&info.id).expect("close");
}

#[test]
fn a_view_that_attaches_is_replayed_the_whole_output() {
    let (manager, sink, dir) = setup();
    let info = manager
        .create(
            &path_of(&dir),
            Some(
                "printf '\\033[?2004h'; i=0; while [ $i -lt 3000 ]; do echo \"line $i\"; i=$((i+1)); done",
            ),
            80,
            24,
        )
        .expect("create");
    sink.wait_for_text(&info.id, "line 2999\r\n");

    let replay = manager.replay(&info.id).expect("replay");
    let full = manager.snapshot(&info.id).expect("snapshot");

    // Nothing was evicted, so the replay is the output itself, the mode included.
    assert_eq!(replay.data, full.data);
    assert_eq!(replay.last_seq, full.last_seq);
    manager.close(&info.id).expect("close");
}

#[test]
fn a_view_that_attaches_after_eviction_gets_the_state_the_dropped_output_set() {
    let (manager, sink, dir) = setup();
    let info = manager
        .create(
            &path_of(&dir),
            Some("printf '\\033[?2004h'; head -c 1200000 /dev/zero | tr '\\0' a; echo MARK-$((6 * 7))"),
            80,
            24,
        )
        .expect("create");
    sink.wait_for_text(&info.id, "MARK-42\r\n");

    let replay = manager.replay(&info.id).expect("replay");

    // The mode was set before the 1.2 MB that evicted it, so the replay sets it again, then the rest.
    assert!(replay.data.starts_with("\x1b[?2004h"));
    assert!(
        replay.data.contains("MARK-42\r\n"),
        "the output after the eviction is replayed"
    );
    assert!(replay.data.len() <= 1024 * 1024 + 64);
    manager.close(&info.id).expect("close");
}
