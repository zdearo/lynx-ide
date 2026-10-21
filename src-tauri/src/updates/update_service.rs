//! Looks for new versions, downloads and installs them, and keeps the status the screen shows.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, SystemTime};

use chrono::{DateTime, Utc};
use tauri::AppHandle;
use tauri_plugin_updater::{Update, UpdaterExt};
use tauri_specta::Event;

use crate::config::config_store::ConfigStore;

use super::update_events::UpdateStatusChanged;
use super::update_state::{UpdatePhase, UpdateStatus, can_self_update, percent};

/// The first check comes shortly after start, so the window is up before anything can show.
const FIRST_CHECK_DELAY: Duration = Duration::from_secs(20);
const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("this copy of Lynx cannot update itself")]
    Unsupported,
    #[error("there is no update to install")]
    NothingToInstall,
    #[error("the update could not be installed: {0}")]
    Install(#[source] tauri_plugin_updater::Error),
}

pub struct UpdateService {
    app: AppHandle,
    supported: bool,
    state: Mutex<State>,
}

struct State {
    current_version: String,
    checked_at: Option<String>,
    phase: UpdatePhase,
    /// The release the last check found. Installing takes it.
    pending: Option<Update>,
}

impl UpdateService {
    pub fn new(app: AppHandle) -> Self {
        let supported =
            can_self_update(std::env::consts::OS, std::env::var_os("APPIMAGE").is_some());
        let phase = if supported {
            UpdatePhase::Idle
        } else {
            UpdatePhase::Unsupported
        };
        let current_version = app.package_info().version.to_string();
        Self {
            app,
            supported,
            state: Mutex::new(State {
                current_version,
                checked_at: None,
                phase,
                pending: None,
            }),
        }
    }

    pub fn status(&self) -> UpdateStatus {
        snapshot(&self.lock())
    }

    /// Looks for a newer release. A failed check is a status the screen shows, not an error.
    pub async fn check(&self) -> UpdateStatus {
        if !self.supported || !self.begin_check() {
            return self.status();
        }

        let result = match self.app.updater() {
            Ok(updater) => updater.check().await,
            Err(error) => Err(error),
        };
        let checked_at = Some(now());
        match result {
            Ok(Some(update)) => {
                let phase = UpdatePhase::Available {
                    version: update.version.clone(),
                    notes: update.body.clone(),
                    date: update
                        .date
                        .and_then(|date| to_rfc3339(date.unix_timestamp())),
                };
                self.finish_check(Some(update), phase, checked_at)
            }
            Ok(None) => self.finish_check(None, UpdatePhase::UpToDate, checked_at),
            Err(error) => {
                eprintln!("lynx: update check failed: {error}");
                self.finish_check(None, UpdatePhase::Failed, checked_at)
            }
        }
    }

    /// Downloads the release the last check found and installs it. Returns once it is in place;
    /// the new version runs after a restart.
    pub async fn install(&self) -> Result<UpdateStatus, UpdateError> {
        if !self.supported {
            return Err(UpdateError::Unsupported);
        }
        let update = self.take_pending()?;
        let version = update.version.clone();

        let mut received: u64 = 0;
        let mut shown: Option<u32> = None;
        let outcome = update
            .download_and_install(
                |chunk, total| {
                    received += chunk as u64;
                    let share = percent(received, total);
                    // Report whole percentages only, not every network chunk.
                    if share != shown {
                        shown = share;
                        self.set_phase(UpdatePhase::Downloading {
                            version: version.clone(),
                            percent: share,
                        });
                    }
                },
                || {},
            )
            .await;

        match outcome {
            Ok(()) => Ok(self.set_phase(UpdatePhase::ReadyToRestart { version })),
            Err(error) => {
                eprintln!("lynx: update install failed: {error}");
                self.set_phase(UpdatePhase::Failed);
                Err(UpdateError::Install(error))
            }
        }
    }

    /// Checks once shortly after start and then every few hours, while automatic checks are on.
    pub fn start_schedule(self: Arc<Self>, config: Arc<ConfigStore>) {
        // A plain thread: one check every few hours does not need a runtime task of its own.
        let _ = std::thread::Builder::new()
            .name("update-checks".to_owned())
            .spawn(move || {
                std::thread::sleep(FIRST_CHECK_DELAY);
                loop {
                    if config.config().updates.check_automatically {
                        tauri::async_runtime::block_on(self.check());
                    }
                    std::thread::sleep(CHECK_INTERVAL);
                }
            });
    }

    /// Marks a check as running. Refused while another check, a download or a finished install
    /// is in progress, so the screen never shows two of them at once.
    fn begin_check(&self) -> bool {
        let mut state = self.lock();
        if matches!(
            state.phase,
            UpdatePhase::Checking
                | UpdatePhase::Downloading { .. }
                | UpdatePhase::ReadyToRestart { .. }
        ) {
            return false;
        }
        state.phase = UpdatePhase::Checking;
        let status = snapshot(&state);
        drop(state);
        self.publish(&status);
        true
    }

    fn finish_check(
        &self,
        pending: Option<Update>,
        phase: UpdatePhase,
        checked_at: Option<String>,
    ) -> UpdateStatus {
        let mut state = self.lock();
        state.pending = pending;
        state.phase = phase;
        state.checked_at = checked_at;
        let status = snapshot(&state);
        drop(state);
        self.publish(&status);
        status
    }

    fn take_pending(&self) -> Result<Update, UpdateError> {
        let mut state = self.lock();
        let update = state.pending.take().ok_or(UpdateError::NothingToInstall)?;
        state.phase = UpdatePhase::Downloading {
            version: update.version.clone(),
            percent: None,
        };
        let status = snapshot(&state);
        drop(state);
        self.publish(&status);
        Ok(update)
    }

    fn set_phase(&self, phase: UpdatePhase) -> UpdateStatus {
        let mut state = self.lock();
        state.phase = phase;
        let status = snapshot(&state);
        drop(state);
        self.publish(&status);
        status
    }

    fn publish(&self, status: &UpdateStatus) {
        let _ = UpdateStatusChanged {
            status: status.clone(),
        }
        .emit(&self.app);
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        // A poisoned lock still holds the last status, which is all this state is.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn snapshot(state: &State) -> UpdateStatus {
    UpdateStatus {
        current_version: state.current_version.clone(),
        checked_at: state.checked_at.clone(),
        phase: state.phase.clone(),
    }
}

fn now() -> String {
    DateTime::<Utc>::from(SystemTime::now()).to_rfc3339()
}

fn to_rfc3339(seconds: i64) -> Option<String> {
    DateTime::<Utc>::from_timestamp(seconds, 0).map(|date| date.to_rfc3339())
}
