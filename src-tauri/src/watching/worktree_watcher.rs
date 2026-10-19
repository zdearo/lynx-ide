//! A native file watcher on one worktree that reports bursts of changes, not every event.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use thiserror::Error;

use super::change_summary::ChangeSummary;
use super::watched_directories::{MAX_WATCHED_DIRECTORIES, directories_below};
use super::worktree_events::WorktreeChanged;

/// How long the files must stay quiet before a burst is reported: an agent writing many files
/// is one update, not hundreds.
const QUIET: Duration = Duration::from_millis(250);
/// A burst that never goes quiet is still reported this often.
const LONGEST_WAIT: Duration = Duration::from_secs(1);
/// How often a stopped watcher notices it was asked to stop.
const STOP_CHECK: Duration = Duration::from_millis(250);

#[derive(Debug, Error)]
pub enum WatchError {
    #[error("the worktree has too many folders to watch")]
    TooLarge,
    #[error("the system refused the watch: {0}")]
    System(#[from] notify::Error),
}

pub type ChangeSink = Arc<dyn Fn(WorktreeChanged) + Send + Sync>;

/// Watching stops when this is dropped.
pub struct WorktreeWatcher {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl WorktreeWatcher {
    /// Watches `root` and hands each settled burst of changes to `sink`, under `reported_path`.
    pub fn start(root: &Path, reported_path: String, sink: ChangeSink) -> Result<Self, WatchError> {
        let (sender, receiver) = mpsc::channel::<notify::Result<Event>>();
        let mut watcher: RecommendedWatcher = notify::recommended_watcher(move |event| {
            let _ = sender.send(event);
        })?;

        let folders =
            directories_below(root, MAX_WATCHED_DIRECTORIES).ok_or(WatchError::TooLarge)?;
        let mut watched = folders.len();
        for folder in &folders {
            watcher.watch(folder, RecursiveMode::NonRecursive)?;
        }
        let git_dir = git_dir_of(root);
        if let Some(git_dir) = &git_dir {
            watcher.watch(git_dir, RecursiveMode::NonRecursive)?;
        }

        let stop = Arc::new(AtomicBool::new(false));
        let root = root.to_owned();
        let thread = thread::Builder::new()
            .name("worktree-watcher".to_owned())
            .spawn({
                let stop = Arc::clone(&stop);
                move || {
                    // The thread owns the watcher: when it returns, the watches are released.
                    while !stop.load(Ordering::Relaxed) {
                        let mut summary = ChangeSummary::default();
                        let Some(first) = wait_for_event(&receiver, &stop) else {
                            break;
                        };
                        let started = Instant::now();
                        let mut next = Some(first);
                        while let Some(event) = next.take() {
                            note(
                                &event,
                                &root,
                                git_dir.as_deref(),
                                &mut summary,
                                &mut watcher,
                                &mut watched,
                            );
                            if started.elapsed() >= LONGEST_WAIT {
                                break;
                            }
                            next = receiver.recv_timeout(QUIET).ok();
                        }
                        if !summary.is_empty() {
                            sink(summary.into_event(&reported_path));
                        }
                    }
                }
            })
            .map_err(|error| WatchError::System(notify::Error::io(error)))?;

        Ok(Self {
            stop,
            thread: Some(thread),
        })
    }
}

impl Drop for WorktreeWatcher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            // The thread wakes within STOP_CHECK; waiting keeps two watchers from overlapping.
            let _ = thread.join();
        }
    }
}

/// The next event, or `None` once told to stop or when the watcher is gone.
fn wait_for_event(
    receiver: &mpsc::Receiver<notify::Result<Event>>,
    stop: &AtomicBool,
) -> Option<notify::Result<Event>> {
    loop {
        if stop.load(Ordering::Relaxed) {
            return None;
        }
        match receiver.recv_timeout(STOP_CHECK) {
            Ok(event) => return Some(event),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return None,
        }
    }
}

fn note(
    event: &notify::Result<Event>,
    root: &Path,
    git_dir: Option<&Path>,
    summary: &mut ChangeSummary,
    watcher: &mut RecommendedWatcher,
    watched: &mut usize,
) {
    // A failed event says nothing about which file changed; the next burst will.
    let Ok(event) = event else { return };
    if matches!(event.kind, EventKind::Access(_)) {
        return;
    }
    for path in &event.paths {
        summary.add(root, git_dir, path);
        if matches!(event.kind, EventKind::Create(_)) && path.is_dir() {
            watch_new_folder(path, watcher, watched);
        }
    }
}

/// A folder made after the watch started has no watch of its own, nor do the ones inside it.
fn watch_new_folder(folder: &Path, watcher: &mut RecommendedWatcher, watched: &mut usize) {
    let room = MAX_WATCHED_DIRECTORIES.saturating_sub(*watched);
    let Some(folders) = directories_below(folder, room) else {
        return;
    };
    for folder in folders {
        // Best effort: an unwatched folder is read again on the slow timer.
        if watcher.watch(&folder, RecursiveMode::NonRecursive).is_ok() {
            *watched += 1;
        }
    }
}

/// Where Git keeps this worktree's index and `HEAD`: `.git` itself, or the folder a linked
/// worktree's `.git` file points to.
fn git_dir_of(root: &Path) -> Option<PathBuf> {
    let dot_git = root.join(".git");
    if dot_git.is_dir() {
        return Some(dot_git);
    }
    let pointer = fs::read_to_string(&dot_git).ok()?;
    let target = Path::new(pointer.strip_prefix("gitdir:")?.trim());
    Some(if target.is_absolute() {
        target.to_owned()
    } else {
        root.join(target)
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::git::test_repository::TestRepository;

    type Seen = Arc<Mutex<Vec<WorktreeChanged>>>;

    fn watch(root: &Path) -> (WorktreeWatcher, Seen) {
        let seen: Seen = Arc::default();
        let sink = {
            let seen = Arc::clone(&seen);
            Arc::new(move |change| seen.lock().expect("seen").push(change)) as ChangeSink
        };
        let watcher = WorktreeWatcher::start(root, "/reported".to_owned(), sink).expect("watch");
        (watcher, seen)
    }

    /// Waits for an event that satisfies `wanted`, since the system delivers them a moment later.
    fn wait_for(seen: &Seen, wanted: impl Fn(&WorktreeChanged) -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if seen.lock().expect("seen").iter().any(&wanted) {
                return true;
            }
            thread::sleep(Duration::from_millis(50));
        }
        false
    }

    #[test]
    fn a_new_file_reports_its_folder_under_the_path_it_was_started_with() {
        let repo = TestRepository::new();
        fs::create_dir_all(repo.path.join("src")).expect("dir");
        let (_watcher, seen) = watch(&repo.path);

        fs::write(repo.path.join("src/new.ts"), "x\n").expect("write");

        assert!(wait_for(&seen, |change| {
            change.worktree_path == "/reported" && change.directories == ["src"]
        }));
    }

    #[test]
    fn a_burst_of_writes_is_one_report() {
        let repo = TestRepository::new();
        let (_watcher, seen) = watch(&repo.path);

        for index in 0..20 {
            fs::write(repo.path.join(format!("file{index}.txt")), "x\n").expect("write");
        }
        assert!(wait_for(&seen, |change| change.directories == [""]));
        thread::sleep(Duration::from_millis(600));

        assert_eq!(seen.lock().expect("seen").len(), 1);
    }

    #[test]
    fn a_folder_made_after_the_watch_started_is_watched_too() {
        let repo = TestRepository::new();
        let (_watcher, seen) = watch(&repo.path);

        fs::create_dir_all(repo.path.join("later/deeper")).expect("dirs");
        thread::sleep(Duration::from_millis(600));
        fs::write(repo.path.join("later/deeper/file.txt"), "x\n").expect("write");

        assert!(wait_for(&seen, |change| {
            change.directories.iter().any(|dir| dir == "later/deeper")
        }));
    }

    #[test]
    fn ignored_folders_stay_quiet() {
        let repo = TestRepository::new();
        fs::write(repo.path.join(".gitignore"), "node_modules/\n").expect("ignore");
        fs::create_dir_all(repo.path.join("node_modules/pkg")).expect("dirs");
        let (_watcher, seen) = watch(&repo.path);

        fs::write(repo.path.join("node_modules/pkg/index.js"), "x\n").expect("write");
        thread::sleep(Duration::from_millis(900));

        assert!(seen.lock().expect("seen").is_empty());
    }

    #[test]
    fn staging_a_file_reports_that_git_moved() {
        let repo = TestRepository::new();
        fs::write(repo.path.join("staged.txt"), "x\n").expect("write");
        let (_watcher, seen) = watch(&repo.path);

        repo.git(&["add", "staged.txt"]);

        assert!(wait_for(&seen, |change| change.git));
    }

    #[test]
    fn a_linked_worktree_reports_git_moves_from_outside_its_folder() {
        let repo = TestRepository::new();
        let linked = repo.path.with_file_name(format!(
            "{}-linked",
            repo.path.file_name().expect("name").to_string_lossy()
        ));
        repo.git(&["worktree", "add", "-b", "task", &linked.to_string_lossy()]);
        fs::write(linked.join("staged.txt"), "x\n").expect("write");
        let (_watcher, seen) = watch(&linked);

        crate::git::git_command::git(&linked)
            .args(["add", "staged.txt"])
            .run()
            .expect("add");

        assert!(wait_for(&seen, |change| change.git));
        let _ = fs::remove_dir_all(&linked);
    }

    #[test]
    fn dropping_the_watcher_stops_the_reports() {
        let repo = TestRepository::new();
        let (watcher, seen) = watch(&repo.path);
        drop(watcher);

        fs::write(repo.path.join("after.txt"), "x\n").expect("write");
        thread::sleep(Duration::from_millis(700));

        assert!(seen.lock().expect("seen").is_empty());
    }
}
