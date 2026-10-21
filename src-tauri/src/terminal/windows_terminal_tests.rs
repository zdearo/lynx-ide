//! Real cmd.exe sessions on ConPTY, covering what a terminal must do on Windows: run and echo,
//! start where asked, resize, report exit, take Ctrl+C, and leave no process behind when closed.
//! Windows-only, like the Unix shell tests.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};

use crate::agents::hook_hub::HookHub;
use crate::browser::SessionOwners;

use super::terminal_events::TerminalEventSink;
use super::terminal_manager::TerminalManager;
use super::terminal_session::TerminalInfo;
use super::terminal_test_support::{
    RecordingSink, WAIT, answer_cursor_request, output_of, path_of, wait_for_plain_text,
};
use super::{Shell, TerminalError};

fn setup() -> (TerminalManager, Arc<RecordingSink>, tempfile::TempDir) {
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
    );
    (manager, sink, tempfile::tempdir().expect("temp dir"))
}

/// Opens a session and answers the cursor request, so cmd can start drawing.
fn open(
    manager: &TerminalManager,
    sink: &RecordingSink,
    cwd: &str,
    command: Option<&str>,
    cols: u16,
    rows: u16,
) -> TerminalInfo {
    let info = manager.create(cwd, command, cols, rows).expect("create");
    answer_cursor_request(sink, manager, &info.id);
    info
}

/// The processes whose image is this name. Reading names is cheap; nothing else is asked for.
fn processes_named(image: &str) -> Vec<sysinfo::Pid> {
    let mut system = System::new();
    system.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    system
        .processes()
        .iter()
        .filter(|(_, process)| process.name().to_string_lossy().eq_ignore_ascii_case(image))
        .map(|(pid, _)| *pid)
        .collect()
}

fn wait_for_processes(image: &str, wanted: impl Fn(usize) -> bool, within: Duration) {
    let deadline = Instant::now() + within;
    while !wanted(processes_named(image).len()) {
        assert!(
            Instant::now() < deadline,
            "{image}: {} still running when the wait ended",
            processes_named(image).len()
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// A program that runs for minutes under a name of its own, so it can be found in the process
/// table and nothing else is mistaken for it. A copy of PING.EXE, which exists on every Windows.
struct Probe {
    image: String,
    /// Starts the probe from a second cmd, so it is a grandchild of the session's shell.
    launcher: PathBuf,
}

impl Probe {
    fn new(dir: &std::path::Path, tag: &str) -> Self {
        let image = format!("lynx_probe_{}_{tag}.exe", std::process::id());
        let bin = dir.join("bin");
        fs::create_dir_all(&bin).expect("bin dir");
        let exe = bin.join(&image);
        let system = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
        fs::copy(
            PathBuf::from(system).join("System32").join("PING.EXE"),
            &exe,
        )
        .expect("copy ping");
        let launcher = bin.join(format!("run_{tag}.cmd"));
        fs::write(
            &launcher,
            format!("@\"{}\" -n 300 127.0.0.1\r\n", exe.display()),
        )
        .expect("write launcher");
        Self { image, launcher }
    }

    /// The line to type: a nested cmd that runs the probe. `""…""` is how cmd /c takes a path
    /// that may hold spaces, as in C:\Users\Some Name.
    fn command(&self) -> String {
        format!("cmd /c \"\"{}\"\"", self.launcher.display())
    }

    fn running(&self) -> usize {
        processes_named(&self.image).len()
    }
}

impl Drop for Probe {
    // A failed test must not leave a five-minute ping on the machine.
    fn drop(&mut self) {
        let mut system = System::new();
        system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing(),
        );
        for pid in processes_named(&self.image) {
            if let Some(process) = system.process(pid) {
                process.kill();
            }
        }
    }
}

#[test]
fn a_command_is_typed_into_cmd_and_its_output_is_streamed() {
    let (manager, sink, dir) = setup();

    // The typed text echoes `%OS%`; only the output has it expanded.
    let info = open(
        &manager,
        &sink,
        &path_of(&dir),
        Some("echo agent-%OS%"),
        80,
        24,
    );

    wait_for_plain_text(&sink, &info.id, "agent-Windows_NT");
    assert!(info.running);
    assert_eq!(info.command.as_deref(), Some("echo agent-%OS%"));
    assert_eq!(info.shell, "cmd");
    manager.close(&info.id).expect("close");
}

#[test]
fn the_shell_starts_in_the_requested_directory() {
    let (manager, sink, dir) = setup();
    let cwd = path_of(&dir);

    let info = open(&manager, &sink, &cwd, Some("echo here-%CD%"), 80, 24);

    wait_for_plain_text(&sink, &info.id, &format!("here-{cwd}"));
    manager.close(&info.id).expect("close");
}

#[test]
fn input_reaches_the_running_shell() {
    let (manager, sink, dir) = setup();
    let info = open(&manager, &sink, &path_of(&dir), None, 80, 24);

    manager.write(&info.id, "echo typed-%OS%\r").expect("write");

    wait_for_plain_text(&sink, &info.id, "typed-Windows_NT");
    manager.close(&info.id).expect("close");
}

#[test]
fn resizing_changes_the_size_the_program_sees() {
    let (manager, sink, dir) = setup();
    let info = open(&manager, &sink, &path_of(&dir), Some("mode con"), 100, 30);
    wait_for_plain_text(&sink, &info.id, "Lines:30Columns:100");

    manager.resize(&info.id, 120, 40).expect("resize");
    manager.write(&info.id, "mode con\r").expect("write");

    wait_for_plain_text(&sink, &info.id, "Lines:40Columns:120");
    manager.close(&info.id).expect("close");
}

#[test]
fn exit_reports_the_status_and_the_session_stays_readable() {
    let (manager, sink, dir) = setup();

    let info = open(
        &manager,
        &sink,
        &path_of(&dir),
        Some("echo last-words & exit 7"),
        80,
        24,
    );

    assert_eq!(sink.wait_for_exit(&info.id), Some(7));
    let listed = manager.list();
    assert_eq!(listed.len(), 1);
    assert!(!listed[0].running);
    assert_eq!(listed[0].exit_code, Some(7));
    wait_for_plain_text(&sink, &info.id, "last-words");
    assert!(
        super::terminal_test_support::squeezed(&manager.snapshot(&info.id).expect("snapshot").data)
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
    let info = open(
        &manager,
        &sink,
        &path_of(&dir),
        Some("echo one & echo two & exit 0"),
        80,
        24,
    );

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
}

#[test]
fn ctrl_c_stops_the_running_program_and_the_shell_stays_usable() {
    let (manager, sink, dir) = setup();
    let info = open(
        &manager,
        &sink,
        &path_of(&dir),
        Some("ping -n 300 127.0.0.1"),
        80,
        24,
    );
    wait_for_plain_text(&sink, &info.id, "Reply from");

    manager.write(&info.id, "\u{3}").expect("interrupt");
    wait_for_plain_text(&sink, &info.id, "Control-C");

    manager.write(&info.id, "echo back-%OS%\r").expect("write");
    wait_for_plain_text(&sink, &info.id, "back-Windows_NT");
    manager.close(&info.id).expect("close");
}

#[test]
fn closing_kills_the_whole_process_tree_not_just_the_shell() {
    let (manager, sink, dir) = setup();
    let probe = Probe::new(dir.path(), "close");
    let info = open(
        &manager,
        &sink,
        &path_of(&dir),
        Some(&probe.command()),
        80,
        24,
    );
    wait_for_processes(&probe.image, |count| count >= 1, WAIT);

    manager.close(&info.id).expect("close");

    wait_for_processes(&probe.image, |count| count == 0, Duration::from_secs(8));
    assert!(manager.list().is_empty());
}

#[test]
fn close_all_leaves_no_process_behind() {
    let (manager, sink, dir) = setup();
    let cwd = path_of(&dir);
    let probes = [
        Probe::new(dir.path(), "all_a"),
        Probe::new(dir.path(), "all_b"),
    ];
    for probe in &probes {
        open(&manager, &sink, &cwd, Some(&probe.command()), 80, 24);
    }
    for probe in &probes {
        wait_for_processes(&probe.image, |count| count >= 1, WAIT);
    }

    manager.close_all();

    assert!(manager.list().is_empty());
    for probe in &probes {
        wait_for_processes(&probe.image, |count| count == 0, Duration::from_secs(8));
        assert_eq!(probe.running(), 0);
    }
}

#[test]
fn closing_a_session_that_already_exited_forgets_it() {
    let (manager, sink, dir) = setup();
    let info = open(&manager, &sink, &path_of(&dir), Some("exit 0"), 80, 24);
    sink.wait_for_exit(&info.id);

    manager.close(&info.id).expect("close");

    assert!(manager.list().is_empty());
}

#[test]
fn bad_requests_are_rejected_without_creating_a_session() {
    let (manager, _sink, dir) = setup();
    let cwd = path_of(&dir);

    assert!(matches!(
        manager.create("C:\\definitely\\not\\here", None, 80, 24),
        Err(TerminalError::InvalidWorkingDirectory(_))
    ));
    assert!(matches!(
        manager.create(&cwd, None, 0, 24),
        Err(TerminalError::InvalidSize)
    ));
    assert!(matches!(
        manager.snapshot("nope"),
        Err(TerminalError::NotFound(_))
    ));
    assert!(matches!(
        manager.close("nope"),
        Err(TerminalError::NotFound(_))
    ));
    assert!(manager.list().is_empty());
}
