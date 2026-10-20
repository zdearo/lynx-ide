//! What the status bar shows about a plan: the windows it limits and how full each one is.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::agents::harness::HarnessId;

/// Up to six hours: the short session window both plans we read use.
const SESSION_MAX_MINUTES: u32 = 6 * 60;
/// Up to eight days: the weekly window, which resets a little after seven days.
const WEEKLY_MAX_MINUTES: u32 = 8 * 24 * 60;
/// Up to forty-five days: the monthly window of a free plan.
const MONTHLY_MAX_MINUTES: u32 = 45 * 24 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum WindowKind {
    Session,
    Weekly,
    Monthly,
    /// A window of a length Lynx does not name; the minutes still say how long it is.
    Other,
}

impl WindowKind {
    /// Classifies by length, not by position: a plan may report only one window, and it may be the monthly one.
    pub fn of_minutes(minutes: Option<u32>) -> Self {
        match minutes {
            Some(minutes) if minutes <= SESSION_MAX_MINUTES => Self::Session,
            Some(minutes) if minutes <= WEEKLY_MAX_MINUTES => Self::Weekly,
            Some(minutes) if minutes <= MONTHLY_MAX_MINUTES => Self::Monthly,
            _ => Self::Other,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlanWindow {
    pub kind: WindowKind,
    /// How long the window lasts, when the source says.
    pub minutes: Option<u32>,
    /// Share of the window already used, rounded to a whole percent and possibly above 100 when the plan allows it.
    pub used_percent: u32,
    /// Unix seconds when the window starts over.
    pub resets_at: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum PlanUsageState {
    /// The windows below are what the provider reported.
    Ready,
    /// The provider is there but not signed in, so there is nothing to read.
    SignInNeeded,
    /// Something went wrong; `reason` says what, in the provider's own words where it gave them.
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlanUsage {
    pub harness: HarnessId,
    /// The plan's name as the provider writes it, such as `free` or `plus`.
    pub plan: Option<String>,
    pub state: PlanUsageState,
    pub reason: Option<String>,
    pub windows: Vec<PlanWindow>,
    /// Unix seconds when these numbers were read.
    pub observed_at: u32,
}

/// A share of a window as a whole percent. Negative or non-finite numbers are not real usage.
pub fn whole_percent(percent: f64) -> u32 {
    if !percent.is_finite() || percent <= 0.0 {
        return 0;
    }
    // Clamped to a range `u32` holds, so the cast cannot truncate.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let rounded = percent.min(f64::from(u32::MAX)).round() as u32;
    rounded
}

/// Unix seconds now. Saturates at `u32::MAX` rather than wrapping, which is not reachable in practice.
pub fn now_seconds() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| u32::try_from(elapsed.as_secs()).ok())
        .unwrap_or(u32::MAX)
}

/// Phrases a provider's CLI uses when nobody is signed in. Matching them is a best guess.
const SIGN_IN_PHRASES: [&str; 5] = ["log in", "login", "sign in", "not logged", "authenticat"];

impl PlanUsage {
    /// A read that failed: a sign-in problem when the reason says so, otherwise shown as it came.
    pub fn from_failure(harness: HarnessId, reason: String, observed_at: u32) -> Self {
        let lowered = reason.to_lowercase();
        if SIGN_IN_PHRASES
            .iter()
            .any(|phrase| lowered.contains(phrase))
        {
            Self::sign_in_needed(harness, observed_at)
        } else {
            Self::unavailable(harness, reason, observed_at)
        }
    }

    pub fn ready(
        harness: HarnessId,
        plan: Option<String>,
        windows: Vec<PlanWindow>,
        observed_at: u32,
    ) -> Self {
        Self {
            harness,
            plan,
            state: PlanUsageState::Ready,
            reason: None,
            windows,
            observed_at,
        }
    }

    pub fn sign_in_needed(harness: HarnessId, observed_at: u32) -> Self {
        Self {
            harness,
            plan: None,
            state: PlanUsageState::SignInNeeded,
            reason: None,
            windows: Vec::new(),
            observed_at,
        }
    }

    pub fn unavailable(harness: HarnessId, reason: String, observed_at: u32) -> Self {
        Self {
            harness,
            plan: None,
            state: PlanUsageState::Unavailable,
            reason: Some(reason),
            windows: Vec::new(),
            observed_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_is_named_by_its_length() {
        assert_eq!(WindowKind::of_minutes(Some(300)), WindowKind::Session);
        assert_eq!(WindowKind::of_minutes(Some(10_080)), WindowKind::Weekly);
        assert_eq!(WindowKind::of_minutes(Some(43_200)), WindowKind::Monthly);
        assert_eq!(
            WindowKind::of_minutes(Some(90 * 24 * 60)),
            WindowKind::Other
        );
        assert_eq!(WindowKind::of_minutes(None), WindowKind::Other);
    }
}
