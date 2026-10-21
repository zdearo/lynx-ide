use lynx_browser_protocol::message::PageEvent;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri_specta::Event;

use super::page_owner::PageOwner;

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
    /// The page the person sees, or None when no tab is shown.
    pub page: Option<u32>,
}

/// Sent whenever the browser's state changes.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct BrowserStateChanged {
    pub state: BrowserState,
}

/// One page of the engine, as the interface lists it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BrowserPageInfo {
    pub id: u32,
    pub owner: PageOwner,
    pub url: String,
    pub title: String,
    pub loading: bool,
    /// The harness of the agent working on this page right now ("claude", "codex"...), or None.
    pub agent: Option<String>,
}

/// The last picture of a page, as a PNG at most 240 pixels wide.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BrowserThumbnail {
    pub page: u32,
    /// Base64 of the PNG, without a data URL prefix.
    pub png: String,
}

/// Sent when the list of pages changes, for every owner.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct BrowserPagesChanged {
    pub pages: Vec<BrowserPageInfo>,
}

/// Sent when a page has a new picture for the list.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct BrowserThumbnailChanged {
    pub page: u32,
    pub png: String,
}

impl BrowserState {
    /// Folds one event of a page in. True when something the interface shows changed.
    pub fn apply(&mut self, event: &PageEvent) -> bool {
        let before = self.clone();
        match event {
            PageEvent::Title { title } => self.title.clone_from(title),
            PageEvent::Address { url } => {
                self.url.clone_from(url);
                self.error = None;
            }
            PageEvent::Loading {
                loading,
                can_go_back,
                can_go_forward,
            } => {
                self.loading = *loading;
                self.can_go_back = *can_go_back;
                self.can_go_forward = *can_go_forward;
            }
            PageEvent::LoadFailed { message, .. } => {
                self.loading = false;
                self.error = Some(message.clone());
            }
            // Frames go to the view, and DevTools answers to whoever asked.
            PageEvent::Frame { .. }
            | PageEvent::CdpResult { .. }
            | PageEvent::CdpEvent { .. }
            | PageEvent::DevToolsAttached
            | PageEvent::Notice { .. } => {}
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
    fn a_repeated_message_changes_nothing() {
        let mut state = BrowserState::default();
        let title = PageEvent::Title {
            title: "Olá".into(),
        };
        assert!(state.apply(&title));
        assert!(!state.apply(&title), "nothing new to tell the interface");
    }

    #[test]
    fn frames_do_not_touch_the_state() {
        let mut state = BrowserState::default();
        assert!(!state.apply(&PageEvent::Frame {
            slot: 0,
            width: 1,
            height: 1
        }));
    }

    #[test]
    fn a_new_address_clears_the_old_error() {
        let mut state = BrowserState::default();
        state.apply(&PageEvent::LoadFailed {
            url: "x".into(),
            message: "ERR_NAME_NOT_RESOLVED".into(),
        });
        assert_eq!(state.error.as_deref(), Some("ERR_NAME_NOT_RESOLVED"));
        state.apply(&PageEvent::Address {
            url: "https://example.com/".into(),
        });
        assert_eq!(state.error, None);
        assert_eq!(state.url, "https://example.com/");
    }

    #[test]
    fn loading_carries_the_history_buttons() {
        let mut state = BrowserState::default();
        state.apply(&PageEvent::Loading {
            loading: true,
            can_go_back: true,
            can_go_forward: false,
        });
        assert!(state.loading && state.can_go_back && !state.can_go_forward);
    }

    #[test]
    fn a_failed_load_stops_the_loading_mark() {
        let mut state = BrowserState::default();
        state.apply(&PageEvent::Loading {
            loading: true,
            can_go_back: false,
            can_go_forward: false,
        });
        state.apply(&PageEvent::LoadFailed {
            url: "x".into(),
            message: "boom".into(),
        });
        assert!(!state.loading);
    }
}
