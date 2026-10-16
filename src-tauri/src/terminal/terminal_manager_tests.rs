//! Runs real shells on a PTY, so these are Unix-only.

use std::fs;
use std::path::Path;
#[cfg(not(target_os = "linux"))]
use std::process::Command;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use super::terminal_events::TerminalEventSink;
use super::terminal_manager::TerminalManager;
use super::{Shell, TerminalError};

const WAIT: Duration = Duration::from_secs(10);

#[derive(Default)]
struct Events {
    output: Vec<(String, u32, String)>,
    exits: Vec<(String, Option<i32>)>,
}

#[derive(Default)]
struct RecordingSink {
    events: Mutex<Events>,
    changed: Condvar,
}

impl TerminalEventSink for RecordingSink {
    fn output(&self, id: &str, seq: u32, data: String) {
        self.events
            .lock()
            .expect("lock")
            .output
            .push((id.to_owned(), seq, data));
        self.changed.notify_all();
    }

    fn exit(&self, id: &str, exit_code: Option<i32>) {
        self.events
            .lock()
            .expect("lock")
            .exits
            .push((id.to_owned(), exit_code));
        self.changed.notify_all();
    }
}

impl RecordingSink {
    fn wait_until(&self, what: &str, condition: impl Fn(&Events) -> bool) {
        let deadline = Instant::now() + WAIT;
        let mut events = self.events.lock().expect("lock");
        while !condition(&events) {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(
                !remaining.is_zero(),
                "timed out waiting for {what}; output so far: {:?}",
                events
                    .output
                    .iter()
                    .map(|(_, _, data)| data.as_str())
                    .collect::<String>()
            );
            events = self
                .changed
                .wait_timeout(events, remaining)
                .expect("wait")
                .0;
        }
    }

    fn wait_for_text(&self, id: &str, needle: &str) {
        self.wait_until(&format!("{needle:?} in the output"), |events| {
            output_of(events, id).contains(needle)
        });
    }

    fn wait_for_exit(&self, id: &str) -> Option<i32> {
        self.wait_until("the exit event", |events| {
            events.exits.iter().any(|(i, _)| i == id)
        });
        let events = self.events.lock().expect("lock");
        events
            .exits
            .iter()
            .find(|(i, _)| i == id)
            .and_then(|(_, code)| *code)
    }
}

fn output_of(events: &Events, id: &str) -> String {
    events
        .output
        .iter()
        .filter(|(i, _, _)| i == id)
        .map(|(_, _, data)| data.as_str())
        .collect()
}

fn setup() -> (TerminalManager, Arc<RecordingSink>, tempfile::TempDir) {
    let sink = Arc::new(RecordingSink::default());
    // A bare `sh` keeps prompts and rc files out of the way.
    let shell = Arc::new(|| Shell {
        program: "/bin/sh".to_owned(),
        args: Vec::new(),
    });
    let manager = TerminalManager::new(shell, Arc::clone(&sink) as Arc<dyn TerminalEventSink>);
    (manager, sink, tempfile::tempdir().expect("temp dir"))
}

fn path_of(dir: &tempfile::TempDir) -> String {
    fs::canonicalize(dir.path())
        .expect("canonical")
        .to_string_lossy()
        .into_owned()
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
