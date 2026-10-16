//! What a terminal's foreground job is, read from the operating system. On Unix the job is the
//! process group that owns the terminal. Windows has no such group, so there the job is what the
//! shell has started, found by walking the process tree down from the shell.

#[cfg(any(windows, test))]
use std::collections::{HashSet, VecDeque};
#[cfg(unix)]
use std::path::Path;
#[cfg(unix)]
use std::process::{Command, Stdio};

#[cfg(any(windows, test))]
use crate::agents::agent_monitor::Foreground;
#[cfg(any(windows, test))]
use crate::agents::harness::harness_of_job;

/// The arguments of a process, as it was started. None when the process no longer exists.
#[cfg(unix)]
pub fn command_line(pid: i32) -> Option<Vec<String>> {
    if Path::new("/proc/self").exists() {
        let raw = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
        let args: Vec<String> = raw
            .split(|byte| *byte == 0)
            .filter(|arg| !arg.is_empty())
            .map(|arg| String::from_utf8_lossy(arg).into_owned())
            .collect();
        return (!args.is_empty()).then_some(args);
    }
    // Elsewhere ps prints the line joined by spaces, so an argument with a space in it is split.
    let output = Command::new("ps")
        .args(["-o", "args=", "-p", &pid.to_string()])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let args: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    (!args.is_empty()).then_some(args)
}

/// One process, as the job search needs it.
#[cfg(any(windows, test))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessRecord {
    pub pid: u32,
    pub parent: Option<u32>,
    /// The program's file name, which tells a console host from the user's program.
    pub name: String,
    /// The arguments, the program first. Empty when they were not read.
    pub argv: Vec<String>,
}

/// Windows starts these for a console, not for the program the user runs.
#[cfg(any(windows, test))]
const CONSOLE_HOSTS: &[&str] = &["conhost.exe", "openconsole.exe"];

/// The processes the shell started, directly or through another, nearest the shell first.
#[cfg(any(windows, test))]
pub fn started_by_shell(shell_pid: u32, processes: &[ProcessRecord]) -> Vec<&ProcessRecord> {
    let mut found = Vec::new();
    let mut seen = HashSet::from([shell_pid]);
    let mut queue = VecDeque::from([shell_pid]);
    while let Some(parent) = queue.pop_front() {
        for process in processes
            .iter()
            .filter(|process| process.parent == Some(parent))
        {
            if seen.insert(process.pid) {
                queue.push_back(process.pid);
                found.push(process);
            }
        }
    }
    found
}

/// What the shell runs. The agent nearest the shell is the job; without one, the nearest program
/// that is not a console host is. With nothing under the shell, the shell itself is in front.
#[cfg(any(windows, test))]
pub fn job_under_shell(shell_pid: u32, processes: &[ProcessRecord]) -> Foreground {
    let programs: Vec<&ProcessRecord> = started_by_shell(shell_pid, processes)
        .into_iter()
        .filter(|process| !is_console_host(&process.name))
        .collect();
    let job = programs
        .iter()
        .find(|process| harness_of_job(&process.argv).is_some())
        .or(programs.first());
    match job {
        Some(process) => Foreground::Job(process.argv.clone()),
        None => Foreground::Shell,
    }
}

#[cfg(any(windows, test))]
fn is_console_host(name: &str) -> bool {
    CONSOLE_HOSTS
        .iter()
        .any(|host| name.eq_ignore_ascii_case(host))
}

/// The job under a Windows shell, read from the process table. The table is cheap to read; command
/// lines are read only for the shell's own processes, since reading them for every process is slow.
#[cfg(windows)]
pub fn shell_job(shell_pid: u32) -> Foreground {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

    let mut system = System::new();
    system.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    let mut records: Vec<ProcessRecord> = system
        .processes()
        .iter()
        .map(|(pid, process)| ProcessRecord {
            pid: pid.as_u32(),
            parent: process.parent().map(Pid::as_u32),
            name: process.name().to_string_lossy().into_owned(),
            argv: Vec::new(),
        })
        .collect();

    let started: HashSet<u32> = started_by_shell(shell_pid, &records)
        .iter()
        .map(|process| process.pid)
        .collect();
    let pids: Vec<Pid> = started.iter().map(|pid| Pid::from_u32(*pid)).collect();
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&pids),
        false,
        ProcessRefreshKind::nothing().with_cmd(UpdateKind::Always),
    );
    for record in records
        .iter_mut()
        .filter(|record| started.contains(&record.pid))
    {
        let Some(process) = system.process(Pid::from_u32(record.pid)) else {
            continue;
        };
        let argv: Vec<String> = process
            .cmd()
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        // Some processes do not show their command line to other users; the name still says what runs.
        record.argv = if argv.is_empty() {
            vec![record.name.clone()]
        } else {
            argv
        };
    }
    job_under_shell(shell_pid, &records)
}

#[cfg(all(test, unix))]
mod unix_tests {
    use super::*;

    #[test]
    fn the_running_test_process_reports_its_own_arguments() {
        let args = command_line(i32::try_from(std::process::id()).expect("pid fits"))
            .expect("own command line");
        assert!(!args.is_empty());
        assert!(args[0].contains("lynx_ide"), "{args:?}");
    }

    #[test]
    fn a_process_that_does_not_exist_has_no_command_line() {
        // Pid 0 is never a user process, so nothing is reported for it.
        assert_eq!(command_line(0), None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process(pid: u32, parent: u32, name: &str, argv: &[&str]) -> ProcessRecord {
        ProcessRecord {
            pid,
            parent: Some(parent),
            name: name.to_owned(),
            argv: argv.iter().map(|arg| (*arg).to_owned()).collect(),
        }
    }

    #[test]
    fn a_shell_with_nothing_under_it_is_in_front() {
        let table = [process(40, 7, "other.exe", &["other.exe"])];
        assert_eq!(job_under_shell(10, &table), Foreground::Shell);
    }

    #[test]
    fn the_agent_nearest_the_shell_is_the_job() {
        // The shell starts a cmd shim, which starts node running the agent's script under a folder
        // with a space in its name.
        let table = [
            process(20, 10, "cmd.exe", &["cmd.exe", "/d", "/c", "claude.cmd"]),
            process(
                30,
                20,
                "node.exe",
                &[
                    r"C:\Program Files\nodejs\node.exe",
                    r"C:\Users\Some Name\AppData\Roaming\npm\claude",
                ],
            ),
        ];
        assert_eq!(
            job_under_shell(10, &table),
            Foreground::Job(vec![
                r"C:\Program Files\nodejs\node.exe".to_owned(),
                r"C:\Users\Some Name\AppData\Roaming\npm\claude".to_owned(),
            ])
        );
    }

    #[test]
    fn a_program_that_is_not_an_agent_is_the_job_without_being_one() {
        let table = [process(20, 10, "npm.exe", &["npm.exe", "test"])];
        assert_eq!(
            job_under_shell(10, &table),
            Foreground::Job(vec!["npm.exe".to_owned(), "test".to_owned()])
        );
    }

    #[test]
    fn console_hosts_do_not_count_as_a_job() {
        let table = [process(20, 10, "conhost.exe", &["conhost.exe", "0x4"])];
        assert_eq!(job_under_shell(10, &table), Foreground::Shell);
    }

    #[test]
    fn processes_that_the_shell_did_not_start_are_ignored() {
        let table = [process(20, 99, "claude.exe", &["claude.exe"])];
        assert_eq!(job_under_shell(10, &table), Foreground::Shell);
    }

    #[test]
    fn a_table_that_loops_back_on_itself_still_ends() {
        // Pids are reused, so a process can name a parent that is one of its own descendants.
        let table = [
            process(20, 10, "a.exe", &["a.exe"]),
            process(10, 20, "b.exe", &["b.exe"]),
        ];
        // The shell is never counted as its own descendant, so the loop adds nothing more.
        assert_eq!(started_by_shell(10, &table).len(), 1);
    }
}
