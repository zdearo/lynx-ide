use lynx_browser_protocol::message::FromHost;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri_specta::Event;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum BrowserStatus {
    /// No engine process: never started, closed, or stopped on purpose.
    #[default]
    Stopped,
    Starting,
    Running,
    /// The engine could not start or went away; `error` says why.
    Failed,
}

/// What the interface shows about the browser: its status and the page it is on.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BrowserState {
    pub status: BrowserStatus,
    pub url: String,
    pub title: String,
    pub loading: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub error: Option<String>,
    /// The page is shown in its own window, not in the main one.
    pub detached: bool,
}

/// Sent whenever the browser's state changes.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct BrowserStateChanged {
    pub state: BrowserState,
}

impl BrowserState {
    pub fn starting() -> Self {
        Self {
            status: BrowserStatus::Starting,
            ..Self::default()
        }
    }

    pub fn failed(error: impl Into<String>) -> Self {
        Self {
            status: BrowserStatus::Failed,
            error: Some(error.into()),
            ..Self::default()
        }
    }

    /// Folds one message from the engine in. True when something the interface shows changed.
    pub fn apply(&mut self, message: &FromHost) -> bool {
        let before = self.clone();
        match message {
            FromHost::Ready => self.status = BrowserStatus::Running,
            FromHost::Title { title } => self.title.clone_from(title),
            FromHost::Address { url } => {
                self.url.clone_from(url);
                self.error = None;
            }
            FromHost::Loading {
                loading,
                can_go_back,
                can_go_forward,
            } => {
                self.loading = *loading;
                self.can_go_back = *can_go_back;
                self.can_go_forward = *can_go_forward;
            }
            FromHost::LoadFailed { message, .. } => {
                self.loading = false;
                self.error = Some(message.clone());
            }
            // Frames go to the view, and DevTools answers to whoever asked.
            FromHost::Frame { .. } | FromHost::CdpResult { .. } => {}
        }
        *self != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_state_is_stopped() {
        assert_eq!(BrowserState::default().status, BrowserStatus::Stopped);
    }

    #[test]
    fn ready_makes_it_running() {
        let mut state = BrowserState::starting();
        assert!(state.apply(&FromHost::Ready));
        assert_eq!(state.status, BrowserStatus::Running);
    }

    #[test]
    fn a_repeated_message_changes_nothing() {
        let mut state = BrowserState::default();
        let title = FromHost::Title {
            title: "Olá".into(),
        };
        assert!(state.apply(&title));
        assert!(!state.apply(&title), "nothing new to tell the interface");
    }

    #[test]
    fn frames_do_not_touch_the_state() {
        let mut state = BrowserState::default();
        assert!(!state.apply(&FromHost::Frame {
            slot: 0,
            width: 1,
            height: 1
        }));
    }

    #[test]
    fn a_new_address_clears_the_old_error() {
        let mut state = BrowserState::default();
        state.apply(&FromHost::LoadFailed {
            url: "x".into(),
            message: "ERR_NAME_NOT_RESOLVED".into(),
        });
        assert_eq!(state.error.as_deref(), Some("ERR_NAME_NOT_RESOLVED"));
        state.apply(&FromHost::Address {
            url: "https://example.com/".into(),
        });
        assert_eq!(state.error, None);
        assert_eq!(state.url, "https://example.com/");
    }

    #[test]
    fn loading_carries_the_history_buttons() {
        let mut state = BrowserState::default();
        state.apply(&FromHost::Loading {
            loading: true,
            can_go_back: true,
            can_go_forward: false,
        });
        assert!(state.loading && state.can_go_back && !state.can_go_forward);
    }

    #[test]
    fn a_failed_load_stops_the_loading_mark() {
        let mut state = BrowserState::default();
        state.apply(&FromHost::Loading {
            loading: true,
            can_go_back: false,
            can_go_forward: false,
        });
        state.apply(&FromHost::LoadFailed {
            url: "x".into(),
            message: "boom".into(),
        });
        assert!(!state.loading);
    }
}
