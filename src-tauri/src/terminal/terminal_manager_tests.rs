//! Runs real shells on a PTY, so these are Unix-only.

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

use super::terminal_events::TerminalEventSink;
use super::terminal_manager::TerminalManager;
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
    );
    (manager, sink, tempfile::tempdir().expect("temp dir"))
}

fn is_running(pid: &str) -> bool {
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

fn read_pid(file: &Path) -> String {
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

fn wait_until_stopped(pid: &str, within: Duration) {
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
fn a_shell_that_runs_grok_becomes_an_agent_and_back_when_it_ends() {
    a_shell_becomes_the_harness_that_reports_from_it_and_back(
        HarnessId::Grok,
        r#"{"hook_event_name":"SessionStart"}"#,
        r#"{"hook_event_name":"SessionEnd"}"#,
    );
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
