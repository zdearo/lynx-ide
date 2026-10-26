//! Measurements, not assertions: `bun run bench:backend` runs them with `--ignored`.
//! They time what one refresh of the Changes tab asks Git for, on a repository with many files
//! (`LYNX_BENCH_FILES`, default 30000) or on any repository (`LYNX_BENCH_REPO`).

use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Instant;

use crate::git::git_command::{git, spawn_count};
use crate::git::test_repository::TestRepository;

use super::{branch_sync, change_list, commit_history};

fn median_ms(mut run: impl FnMut()) -> f64 {
    run(); // warm: the first read fills the file system cache
    let mut samples: Vec<f64> = (0..7)
        .map(|_| {
            let started = Instant::now();
            run();
            started.elapsed().as_secs_f64() * 1000.0
        })
        .collect();
    samples.sort_by(f64::total_cmp);
    samples[samples.len() / 2]
}

/// A repository with `files` tracked files, an `origin` that is in step, a few edited files,
/// some staged ones and some new ones: what an agent leaves behind.
fn big_repository(files: usize) -> TestRepository {
    let repo = TestRepository::new().with_origin();
    for number in 0..files {
        let directory = repo.path.join(format!("module-{:03}", number % 300));
        fs::create_dir_all(&directory).expect("directory");
        let body: String = (0..30)
            .map(|line| format!("line {line} of {number}\n"))
            .collect();
        fs::write(directory.join(format!("file-{number}.rs")), body).expect("file");
    }
    repo.git(&["add", "-A"]);
    repo.git(&[
        "-c",
        "user.name=Test",
        "-c",
        "user.email=test@example.com",
        "-c",
        "commit.gpgsign=false",
        "commit",
        "--quiet",
        "-m",
        "many files",
    ]);
    repo.git(&["push", "--quiet", "origin", "main"]);
    for number in 0..20 {
        let path = repo
            .path
            .join(format!("module-{:03}/file-{number}.rs", number % 300));
        fs::write(path, "edited\n").expect("edit");
    }
    repo.git(&["add", "module-000/file-0.rs", "module-001/file-1.rs"]);
    for number in 0..100 {
        fs::write(repo.path.join(format!("new-{number}.rs")), "fresh\nfile\n").expect("new");
    }
    repo
}

fn measure(name: &str, root: &Path) {
    let timed = |args: &[&str]| {
        median_ms(|| {
            let _ = git(root).args(args).run_bytes();
        })
    };
    let status_branch = timed(&[
        "--no-optional-locks",
        "status",
        "--porcelain=v2",
        "--branch",
        "--untracked-files=no",
    ]);
    let status_all = timed(&[
        "--no-optional-locks",
        "status",
        "--porcelain=v1",
        "-z",
        "--untracked-files=all",
    ]);
    let numstat = timed(&["--no-optional-locks", "diff", "--numstat", "-z", "-M"]);
    let log = timed(&["log", "-n", "50", "--format=%H"]);
    // Candidates: the counts limited to the files `status` named, and the branch read from refs.
    let changed: Vec<String> = change_list::list_changes(root)
        .map(|files| files.into_iter().map(|file| file.path).collect())
        .unwrap_or_default();
    let numstat_limited = median_ms(|| {
        let _ = git(root)
            .args(["--no-optional-locks", "diff", "--numstat", "-z", "-M", "--"])
            .args(&changed)
            .run_bytes();
    });
    let branch_refs = median_ms(|| {
        let _ = git(root)
            .args(["symbolic-ref", "-q", "--short", "HEAD"])
            .run();
        let _ = git(root)
            .args([
                "for-each-ref",
                "--format=%(upstream:short) %(upstream:track)",
                "refs/heads/main",
            ])
            .run();
    });
    let remote = timed(&["remote"]);

    let numstat_staged = timed(&[
        "--no-optional-locks",
        "diff",
        "--numstat",
        "-z",
        "-M",
        "--cached",
    ]);

    // Exact, unlike the times: how many git processes each read starts.
    let procs = |run: &dyn Fn()| {
        let before = spawn_count::started_in(root);
        run();
        (spawn_count::started_in(root) - before) as f64
    };
    let changes_procs = procs(&|| {
        let _ = change_list::list_changes(root);
    });
    let branch_procs = procs(&|| {
        let _ = branch_sync::read(root);
    });
    let commits_procs = procs(&|| {
        let _ = commit_history::list(root, 50);
    });

    let list_changes = median_ms(|| {
        let _ = change_list::list_changes(root);
    });
    let branch = median_ms(|| {
        let _ = branch_sync::read(root);
    });
    let commits = median_ms(|| {
        let _ = commit_history::list(root, 50);
    });

    // The tab asks for the three together; they run on separate threads.
    let owned: PathBuf = root.to_path_buf();
    let together = median_ms(|| {
        thread::scope(|scope| {
            scope.spawn(|| {
                let _ = change_list::list_changes(&owned);
            });
            scope.spawn(|| {
                let _ = branch_sync::read(&owned);
            });
            scope.spawn(|| {
                let _ = commit_history::list(&owned, 50);
            });
        });
    });

    println!(
        "BENCH {name} status_branch_ms={status_branch:.2} status_all_ms={status_all:.2} \
         numstat_ms={numstat:.2} numstat_staged_ms={numstat_staged:.2} numstat_limited_ms={numstat_limited:.2} \
         branch_refs_ms={branch_refs:.2} log_ms={log:.2} remote_ms={remote:.2} \
         list_changes_ms={list_changes:.2} branch_sync_ms={branch:.2} list_commits_ms={commits:.2} \
         sum_ms={:.2} refresh_wall_ms={together:.2} \
         procs_changes={changes_procs} procs_branch={branch_procs} procs_commits={commits_procs}",
        list_changes + branch + commits
    );
}

#[test]
#[ignore = "measurement"]
fn bench_changes_refresh_many_files() {
    let files = std::env::var("LYNX_BENCH_FILES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(30_000);
    let repo = big_repository(files);
    measure("changes_refresh", &repo.path);
}

#[test]
#[ignore = "measurement"]
fn bench_changes_refresh_given_repository() {
    let Ok(path) = std::env::var("LYNX_BENCH_REPO") else {
        return;
    };
    measure("changes_refresh_real", Path::new(&path));
}
