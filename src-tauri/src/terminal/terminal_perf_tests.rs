//! Measurements, not assertions: `bun run bench:backend` runs them with `--ignored`.
//! Each prints `BENCH <name> key=value ...` lines that `scripts/bench.mjs` collects.

use std::collections::BTreeMap;
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::agents::hook_hub::HookHub;
use crate::browser::SessionOwners;

use super::Shell;
use super::output_channels::output_frame;
use super::terminal_events::TerminalEventSink;
use super::terminal_manager::TerminalManager;

/// What an output event looked like before output moved to channels, to size the JSON it cost.
#[derive(serde::Serialize)]
struct LegacyOutputEvent {
    id: String,
    seq: u32,
    data: String,
}

/// Counts what the webview would be sent. Serializing each event as JSON is what the IPC does
/// before the page sees it, so the cost of many small events shows up here too.
#[derive(Default)]
struct CountingSink {
    events: AtomicU64,
    payload_bytes: AtomicU64,
    raw_bytes: AtomicU64,
    output_bytes: AtomicU64,
    exits: Mutex<Vec<String>>,
    changed: Condvar,
    echoes: Mutex<Vec<(String, Instant)>>,
}

impl TerminalEventSink for CountingSink {
    fn output(&self, id: &str, seq: u32, data: String) {
        self.output_bytes
            .fetch_add(data.len() as u64, Ordering::Relaxed);
        let echo = data.contains("PING");
        let event = LegacyOutputEvent {
            id: id.to_owned(),
            seq,
            data,
        };
        let json = serde_json::to_string(&event).unwrap_or_default();
        self.payload_bytes
            .fetch_add(json.len() as u64, Ordering::Relaxed);
        self.raw_bytes.fetch_add(
            output_frame(event.seq, &event.data).len() as u64,
            Ordering::Relaxed,
        );
        self.events.fetch_add(1, Ordering::Relaxed);
        if echo {
            self.echoes
                .lock()
                .expect("lock")
                .push((id.to_owned(), Instant::now()));
            self.changed.notify_all();
        }
    }

    fn exit(&self, id: &str, _exit_code: Option<i32>) {
        self.exits.lock().expect("lock").push(id.to_owned());
        self.changed.notify_all();
    }
}

impl CountingSink {
    fn wait_for_exits(&self, ids: &[String], within: Duration) -> bool {
        let deadline = Instant::now() + within;
        let mut exits = self.exits.lock().expect("lock");
        while !ids.iter().all(|id| exits.contains(id)) {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return false;
            }
            exits = self.changed.wait_timeout(exits, remaining).expect("wait").0;
        }
        true
    }
}

fn setup() -> (TerminalManager, Arc<CountingSink>, tempfile::TempDir) {
    let sink = Arc::new(CountingSink::default());
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

/// CPU time (user + system) of this process, in milliseconds.
fn cpu_ms() -> f64 {
    let stat = fs::read_to_string("/proc/self/stat").unwrap_or_default();
    let after_name = stat.rsplit(") ").next().unwrap_or("");
    let fields: Vec<&str> = after_name.split_whitespace().collect();
    // After the name: state is field 0, utime is 11 and stime is 12.
    let ticks: f64 = [11, 12]
        .iter()
        .filter_map(|&i| fields.get(i)?.parse::<f64>().ok())
        .sum();
    ticks * 1000.0 / 100.0
}

/// Resident memory of this process, in megabytes.
fn rss_mb() -> f64 {
    let status = fs::read_to_string("/proc/self/status").unwrap_or_default();
    status
        .lines()
        .find(|line| line.starts_with("VmRSS:"))
        .and_then(|line| line.split_whitespace().nth(1)?.parse::<f64>().ok())
        .map_or(f64::NAN, |kilobytes| kilobytes / 1024.0)
}

fn thread_count() -> usize {
    fs::read_dir("/proc/self/task").map_or(0, |dir| dir.count())
}

/// Voluntary context switches over all threads: a thread that wakes up on a timer adds to it.
fn wakeups() -> u64 {
    let Ok(tasks) = fs::read_dir("/proc/self/task") else {
        return 0;
    };
    tasks
        .flatten()
        .filter_map(|task| fs::read_to_string(task.path().join("status")).ok())
        .flat_map(|status| {
            status
                .lines()
                .find(|line| line.starts_with("voluntary_ctxt_switches"))
                .and_then(|line| line.split_whitespace().nth(1)?.parse::<u64>().ok())
        })
        .sum()
}

fn report(name: &str, values: &[(&str, f64)]) {
    let pairs: BTreeMap<_, _> = values.iter().copied().collect();
    let line = pairs
        .iter()
        .map(|(key, value)| format!("{key}={value:.2}"))
        .collect::<Vec<_>>()
        .join(" ");
    println!("BENCH {name} {line}");
}

/// Three sessions print 20 MB each, while five more sit idle: an agent flooding its log.
#[test]
#[ignore = "measurement"]
fn bench_output_flood() {
    let line = "x".repeat(79);
    flood(
        "output_flood",
        &format!("yes {line} | head -c 20000000; exit"),
    );
}

/// The same flood, as a program with colours prints it: every escape is six bytes as JSON.
#[test]
#[ignore = "measurement"]
fn bench_output_flood_ansi() {
    let line = "$(printf '\\033[1;32mok\\033[0m  src/file.rs \\033[2m12ms\\033[0m \\033[33mwarn\\033[0m unused')";
    flood(
        "output_flood_ansi",
        &format!("yes \"{line}\" | head -c 20000000; exit"),
    );
}

fn flood(name: &str, flood: &str) {
    let (manager, sink, dir) = setup();
    let cwd = dir.path().to_string_lossy().into_owned();
    for _ in 0..5 {
        manager.create(&cwd, None, 120, 40).expect("idle");
    }
    let cpu_before = cpu_ms();
    let started = Instant::now();
    let flooding: Vec<String> = (0..3)
        .map(|_| {
            manager
                .create(&cwd, Some(flood), 120, 40)
                .expect("flood")
                .id
        })
        .collect();
    assert!(
        sink.wait_for_exits(&flooding, Duration::from_secs(120)),
        "the floods never ended"
    );
    let seconds = started.elapsed().as_secs_f64();
    let events = sink.events.load(Ordering::Relaxed) as f64;
    let output = sink.output_bytes.load(Ordering::Relaxed) as f64;
    let payload = sink.payload_bytes.load(Ordering::Relaxed) as f64;
    report(
        name,
        &[
            ("seconds", seconds),
            ("events", events),
            ("events_per_s", events / seconds),
            ("avg_bytes_per_event", output / events.max(1.0)),
            ("output_mb", output / 1e6),
            ("payload_mb", payload / 1e6),
            (
                "raw_frame_mb",
                sink.raw_bytes.load(Ordering::Relaxed) as f64 / 1e6,
            ),
            ("cpu_ms", cpu_ms() - cpu_before),
            ("threads", thread_count() as f64),
        ],
    );
    manager.close_all();
}

/// Twenty idle shells: what the app costs while nothing happens.
#[test]
#[ignore = "measurement"]
fn bench_idle_sessions() {
    let (manager, _sink, dir) = setup();
    let cwd = dir.path().to_string_lossy().into_owned();
    for _ in 0..20 {
        manager.create(&cwd, None, 120, 40).expect("idle");
    }
    // Let the shells print their prompts and the monitors settle.
    std::thread::sleep(Duration::from_secs(3));
    let (cpu_start, wake_start, started) = (cpu_ms(), wakeups(), Instant::now());
    std::thread::sleep(Duration::from_secs(10));
    let seconds = started.elapsed().as_secs_f64();
    report(
        "idle_sessions",
        &[
            ("sessions", 20.0),
            ("threads", thread_count() as f64),
            ("cpu_ms_per_s", (cpu_ms() - cpu_start) / seconds),
            ("wakeups_per_s", (wakeups() - wake_start) as f64 / seconds),
        ],
    );
    manager.close_all();
}

/// How long a typed command takes to come back, while another session floods.
#[test]
#[ignore = "measurement"]
fn bench_echo_latency_under_load() {
    let (manager, sink, dir) = setup();
    let cwd = dir.path().to_string_lossy().into_owned();
    let line = "x".repeat(79);
    let flood = format!("yes {line} | head -c 60000000; exit");
    let busy = manager.create(&cwd, Some(&flood), 120, 40).expect("flood");
    let typing = manager.create(&cwd, None, 120, 40).expect("typing");
    std::thread::sleep(Duration::from_millis(500));
    let mut samples = Vec::new();
    for _ in 0..40 {
        let before = sink.echoes.lock().expect("lock").len();
        let sent = Instant::now();
        manager.write(&typing.id, "echo PI''NG\r").expect("write");
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let found = sink.echoes.lock().expect("lock")[before..]
                .iter()
                .find(|(id, _)| *id == typing.id)
                .map(|(_, at)| *at);
            if let Some(at) = found {
                samples.push(at.duration_since(sent).as_secs_f64() * 1000.0);
                break;
            }
            if Instant::now() > deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        std::thread::sleep(Duration::from_millis(60));
    }
    samples.sort_by(f64::total_cmp);
    let pick = |q: f64| {
        samples
            .get(((samples.len() as f64 * q) as usize).min(samples.len().saturating_sub(1)))
            .copied()
            .unwrap_or(f64::NAN)
    };
    report(
        "echo_latency",
        &[
            ("samples", samples.len() as f64),
            ("p50_ms", pick(0.5)),
            ("p95_ms", pick(0.95)),
            ("max_ms", pick(1.0)),
        ],
    );
    let _ = busy;
    manager.close_all();
}

/// Twenty sessions that each printed 3 MB and stay open: what the backend keeps for them.
#[test]
#[ignore = "measurement"]
fn bench_memory_sessions() {
    let (manager, sink, dir) = setup();
    let cwd = dir.path().to_string_lossy().into_owned();
    let line = "x".repeat(79);
    let flood = format!("yes {line} | head -c 3000000; exit");
    let before = rss_mb();
    let ids: Vec<String> = (0..20)
        .map(|_| {
            manager
                .create(&cwd, Some(&flood), 120, 40)
                .expect("flood")
                .id
        })
        .collect();
    assert!(sink.wait_for_exits(&ids, Duration::from_secs(120)));
    std::thread::sleep(Duration::from_secs(1));
    let held = rss_mb();
    // The shells have exited, but each session keeps its scrollback so a view can attach.
    report(
        "memory_sessions",
        &[
            ("sessions", 20.0),
            ("rss_before_mb", before),
            ("rss_after_mb", held),
            ("mb_per_session", (held - before) / 20.0),
        ],
    );
    manager.close_all();
}

/// Sessions opened and closed in rounds: threads and memory must come back down each time.
#[test]
#[ignore = "measurement"]
fn bench_session_churn() {
    let (manager, _sink, dir) = setup();
    let cwd = dir.path().to_string_lossy().into_owned();
    let mut threads_after = Vec::new();
    let mut rss_after = Vec::new();
    for _ in 0..4 {
        let ids: Vec<String> = (0..40)
            .map(|_| {
                manager
                    .create(&cwd, Some("echo hello"), 120, 40)
                    .expect("create")
                    .id
            })
            .collect();
        std::thread::sleep(Duration::from_millis(500));
        for id in &ids {
            manager.close(id).expect("close");
        }
        // Reapers and drain graces take a few seconds to finish.
        std::thread::sleep(Duration::from_secs(5));
        threads_after.push(thread_count() as f64);
        rss_after.push(rss_mb());
    }
    report(
        "session_churn",
        &[
            ("threads_round1", threads_after[0]),
            ("threads_round4", threads_after[3]),
            ("rss_round1_mb", rss_after[0]),
            ("rss_round4_mb", rss_after[3]),
        ],
    );
}

/// Rounds of floods that are then closed, as a long working day is: does memory come back?
#[test]
#[ignore = "measurement"]
fn bench_memory_soak() {
    let (manager, sink, dir) = setup();
    let cwd = dir.path().to_string_lossy().into_owned();
    let line = "x".repeat(79);
    let flood = format!("yes {line} | head -c 10000000; exit");
    let mut after_round = Vec::new();
    for _ in 0..8 {
        let ids: Vec<String> = (0..8)
            .map(|_| {
                manager
                    .create(&cwd, Some(&flood), 120, 40)
                    .expect("flood")
                    .id
            })
            .collect();
        assert!(sink.wait_for_exits(&ids, Duration::from_secs(120)));
        for id in &ids {
            manager.close(id).expect("close");
        }
        std::thread::sleep(Duration::from_secs(2));
        after_round.push(rss_mb());
    }
    report(
        "memory_soak",
        &[
            ("rss_round1_mb", after_round[0]),
            ("rss_round8_mb", after_round[7]),
            (
                "rss_peak_mb",
                after_round.iter().copied().fold(0.0, f64::max),
            ),
        ],
    );
}
