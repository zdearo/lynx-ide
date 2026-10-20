//! Keeps the last plan limits read, so the footer does not start a CLI on every render.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use crate::agents::harness::HarnessId;

use super::claude_usage;
use super::codex_usage;
use super::plan_usage::PlanUsage;
use super::provider_protocol::ProviderCommand;

/// Readings newer than this are reused. Focusing the window and the poll both ask for a fresh snapshot.
const CACHE_TTL: Duration = Duration::from_secs(60);
/// Both CLIs answer in a few seconds; one slower than this is treated as down.
const READ_TIMEOUT: Duration = Duration::from_secs(20);

/// When a plan was read, and what it reported (`None` when its CLI is not installed).
type Reading = (Instant, Option<PlanUsage>);

#[derive(Clone)]
pub struct PlanUsageService {
    codex: ProviderCommand,
    claude: ProviderCommand,
    /// Held while reading, so two asks do not start two rounds of CLIs.
    readings: Arc<Mutex<HashMap<HarnessId, Reading>>>,
}

impl PlanUsageService {
    pub fn new(codex: ProviderCommand, claude: ProviderCommand) -> Self {
        Self {
            codex,
            claude,
            readings: Arc::default(),
        }
    }

    /// The plans asked for, in the order asked. Only the plans whose reading is stale are read again,
    /// all at once. A forced snapshot reads every plan again.
    pub fn snapshot(&self, providers: &[HarnessId], force: bool) -> Vec<PlanUsage> {
        let mut readings = self.readings.lock().unwrap_or_else(PoisonError::into_inner);
        let stale: Vec<HarnessId> = providers
            .iter()
            .copied()
            .filter(|harness| {
                force
                    || !readings
                        .get(harness)
                        .is_some_and(|(taken, _)| taken.elapsed() < CACHE_TTL)
            })
            .collect();

        let fresh: Vec<(HarnessId, Option<PlanUsage>)> = thread::scope(|scope| {
            let running: Vec<(HarnessId, _)> = stale
                .iter()
                .map(|&harness| (harness, scope.spawn(move || self.read(harness))))
                .collect();
            running
                .into_iter()
                .map(|(harness, handle)| (harness, handle.join().ok().flatten()))
                .collect()
        });

        let taken = Instant::now();
        for (harness, usage) in fresh {
            readings.insert(harness, (taken, usage));
        }
        providers
            .iter()
            .filter_map(|harness| readings.get(harness).and_then(|(_, usage)| usage.clone()))
            .collect()
    }

    fn read(&self, harness: HarnessId) -> Option<PlanUsage> {
        match harness {
            HarnessId::Claude => claude_usage::read(&self.claude, READ_TIMEOUT),
            HarnessId::Codex => codex_usage::read(&self.codex, READ_TIMEOUT),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::fs;

    use super::*;

    #[cfg(unix)]
    #[test]
    fn a_fresh_reading_is_reused_and_a_forced_snapshot_reads_again() {
        let dir = tempfile::tempdir().expect("dir");
        let log = dir.path().join("reads.log");
        let script = dir.path().join("fake-codex.sh");
        fs::write(
            &script,
            format!(
                "echo read >> '{}'\nread -r _init\nprintf '%s\\n' '{{\"id\":1,\"result\":{{}}}}'\nread -r _a\nread -r _b\nprintf '%s\\n' '{{\"id\":2,\"result\":{{\"rateLimits\":null}}}}'\n",
                log.display()
            ),
        )
        .expect("write script");
        let service = PlanUsageService::new(
            ProviderCommand::new("sh", &[script.to_str().expect("utf8")]),
            ProviderCommand::new("lynx-no-such-claude-cli", &[]),
        );

        service.snapshot(&[HarnessId::Codex], false);
        service.snapshot(&[HarnessId::Codex], false);
        assert_eq!(fs::read_to_string(&log).expect("log").lines().count(), 1);

        service.snapshot(&[HarnessId::Codex], true);
        assert_eq!(fs::read_to_string(&log).expect("log").lines().count(), 2);
    }

    #[test]
    fn a_plan_that_is_not_installed_shows_nothing_and_a_plan_not_asked_for_is_not_read() {
        let service = PlanUsageService::new(
            ProviderCommand::new("lynx-no-such-codex-cli", &["app-server"]),
            ProviderCommand::new("lynx-no-such-claude-cli", &[]),
        );

        assert!(service.snapshot(&[HarnessId::Claude], false).is_empty());
        assert!(service.snapshot(&[], false).is_empty());
    }
}
