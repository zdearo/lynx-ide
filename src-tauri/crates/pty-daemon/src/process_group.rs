//! Hanging up a process group, then killing what ignores it. Unix only: Windows has no groups.

use std::time::{Duration, Instant};

use nix::sys::signal::{Signal, killpg};
use nix::unistd::Pid;

/// Time a hung-up process gets to exit before it is killed outright.
const HANGUP_GRACE: Duration = Duration::from_secs(3);

/// Hangs up the groups now, and kills the ones still alive after the grace period on a thread.
/// It watches the groups, not the shell: a foreground program can outlive a shell that exited.
pub fn hang_up_and_reap(groups: Vec<i32>) {
    signal_groups(&groups, Signal::SIGHUP);
    // A failed spawn only loses the SIGKILL fallback, so there is nothing to report.
    let _ = std::thread::Builder::new()
        .name("pty-reaper".to_owned())
        .spawn(move || escalate_to_kill(&groups));
}

fn escalate_to_kill(groups: &[i32]) {
    let deadline = Instant::now() + HANGUP_GRACE;
    while Instant::now() < deadline {
        if !groups.iter().copied().any(group_exists) {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let survivors: Vec<i32> = groups
        .iter()
        .copied()
        .filter(|group| group_exists(*group))
        .collect();
    signal_groups(&survivors, Signal::SIGKILL);
}

fn group_exists(group: i32) -> bool {
    // Signal 0 only checks; EPERM still means the group exists.
    !matches!(
        killpg(Pid::from_raw(group), None),
        Err(nix::errno::Errno::ESRCH)
    )
}

/// Freezes the groups, so their programs use no CPU until `resume`.
pub fn stop(groups: &[i32]) {
    signal_groups(groups, Signal::SIGSTOP);
}

/// Lets frozen groups run again.
pub fn resume(groups: &[i32]) {
    signal_groups(groups, Signal::SIGCONT);
}

fn signal_groups(groups: &[i32], signal: Signal) {
    for group in groups {
        // The group may already be gone, which is exactly the outcome we want.
        let _ = killpg(Pid::from_raw(*group), signal);
    }
}
