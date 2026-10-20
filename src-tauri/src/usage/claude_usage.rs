//! Reads the Claude Code plan limits through its own `stream-json` control protocol, the one the Agent SDK speaks.
//! Claude Code answers `get_usage` for the account it is signed in with. Nothing is sent to a model, so no tokens are spent,
//! and Lynx never reads the login token or changes Claude's settings.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::time::{Duration, Instant};

use chrono::DateTime;
use serde_json::{Value, json};

use crate::agents::harness::HarnessId;

use super::plan_usage::{PlanUsage, PlanWindow, WindowKind, now_seconds, whole_percent};
use super::provider_protocol::{ProviderCommand, lines_of, send, stop, take_input, wait_for};

const PROVIDER: &str = "Claude Code";
const REQUEST_ID: &str = "lynx-plan-usage";
/// The windows Claude Code reports, with their lengths in minutes.
const SESSION_MINUTES: u32 = 5 * 60;
const WEEKLY_MINUTES: u32 = 7 * 24 * 60;

/// Settings for the probe alone: the user's hooks, such as a SessionStart one, must not run every few minutes.
const PROBE_SETTINGS: &str = r#"{"disableAllHooks":true}"#;

fn probe_settings_path() -> PathBuf {
    std::env::temp_dir()
        .join("lynx-usage")
        .join("claude-probe-settings.json")
}

fn write_probe_settings(path: &Path) -> io::Result<()> {
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder)?;
    }
    fs::write(path, PROBE_SETTINGS)
}

/// The `stream-json` command of the installed Claude Code CLI. It answers control requests without a prompt.
pub fn installed() -> ProviderCommand {
    let settings = probe_settings_path();
    let settings = settings.to_string_lossy();
    ProviderCommand::new(
        "claude",
        &[
            "-p",
            "--settings",
            &settings,
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--verbose",
        ],
    )
}

/// The plan limits Claude Code reports. `None` when Claude is not installed, or the account has no plan
/// (an API key), since there is then no window to show.
pub fn read(command: &ProviderCommand, timeout: Duration) -> Option<PlanUsage> {
    if !command.is_installed() {
        return None;
    }
    let observed_at = now_seconds();
    let started = write_probe_settings(&probe_settings_path())
        .and_then(|()| command.spawn())
        .map_err(|error| error.to_string());
    let mut child = match started {
        Ok(child) => child,
        Err(reason) => {
            return Some(PlanUsage::from_failure(
                HarnessId::Claude,
                reason,
                observed_at,
            ));
        }
    };
    let outcome = exchange(&mut child, timeout);
    stop(&mut child);

    match outcome {
        Ok(body) => parse_usage(&body, observed_at),
        Err(reason) => Some(PlanUsage::from_failure(
            HarnessId::Claude,
            reason,
            observed_at,
        )),
    }
}

/// Sends the `get_usage` request and returns the answer's body.
fn exchange(child: &mut Child, timeout: Duration) -> Result<Value, String> {
    let deadline = Instant::now() + timeout;
    let mut stdin = take_input(child)?;
    let lines = lines_of(child)?;

    send(
        &mut stdin,
        &json!({
            "type": "control_request",
            "request_id": REQUEST_ID,
            "request": { "subtype": "get_usage" }
        }),
    )?;
    let answer = wait_for(&lines, deadline, |message| {
        message["type"] == "control_response" && message["response"]["request_id"] == REQUEST_ID
    })
    .map_err(|silence| silence.describe(PROVIDER))?;

    let response = &answer["response"];
    if response["subtype"] == "error" {
        return Err(response["error"]
            .as_str()
            .unwrap_or("Claude Code returned an error")
            .to_owned());
    }
    Ok(response["response"].clone())
}

/// Turns the `get_usage` body into what the status bar shows. `None` when the account has no plan limits.
pub fn parse_usage(body: &Value, observed_at: u32) -> Option<PlanUsage> {
    if body["rate_limits_available"] != true {
        return None;
    }
    let limits = &body["rate_limits"];
    let windows: Vec<PlanWindow> = [
        (&limits["five_hour"], SESSION_MINUTES),
        (&limits["seven_day"], WEEKLY_MINUTES),
    ]
    .into_iter()
    .filter_map(|(value, minutes)| window_of(value, minutes))
    .collect();
    if windows.is_empty() {
        return Some(PlanUsage::unavailable(
            HarnessId::Claude,
            "Claude Code reported no limit windows".to_owned(),
            observed_at,
        ));
    }
    let plan = body["subscription_type"].as_str().map(str::to_owned);
    Some(PlanUsage::ready(
        HarnessId::Claude,
        plan,
        windows,
        observed_at,
    ))
}

fn window_of(value: &Value, minutes: u32) -> Option<PlanWindow> {
    let used_percent = whole_percent(value["utilization"].as_f64()?);
    let resets_at = value["resets_at"].as_str().and_then(reset_seconds);
    Some(PlanWindow {
        kind: WindowKind::of_minutes(Some(minutes)),
        minutes: Some(minutes),
        used_percent,
        resets_at,
    })
}

/// Claude gives reset times as RFC 3339 with an offset, such as `2026-10-20T12:00:00.390445+00:00`.
fn reset_seconds(text: &str) -> Option<u32> {
    let time = DateTime::parse_from_rfc3339(text).ok()?;
    u32::try_from(time.timestamp()).ok()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::super::plan_usage::PlanUsageState;
    use super::*;

    /// What `get_usage` answered for a Pro account, with the parts Lynx does not read cut.
    const PRO_ACCOUNT: &str = r#"{
        "rate_limits_available": true,
        "subscription_type": "pro",
        "rate_limits": {
            "five_hour": { "utilization": 17, "resets_at": "2026-10-20T12:00:00.390445+00:00" },
            "seven_day": { "utilization": 5, "resets_at": "2026-10-26T20:00:00.390463+00:00" },
            "seven_day_opus": null
        }
    }"#;

    fn body(text: &str) -> Value {
        serde_json::from_str(text).expect("json")
    }

    #[test]
    fn a_pro_account_shows_its_five_hour_and_weekly_windows() {
        let usage = parse_usage(&body(PRO_ACCOUNT), 100).expect("a plan");

        assert_eq!(usage.state, PlanUsageState::Ready);
        assert_eq!(usage.plan.as_deref(), Some("pro"));
        assert_eq!(usage.windows.len(), 2);
        assert_eq!(usage.windows[0].kind, WindowKind::Session);
        assert_eq!(usage.windows[0].used_percent, 17);
        assert_eq!(usage.windows[1].kind, WindowKind::Weekly);
        assert_eq!(usage.windows[1].used_percent, 5);
    }

    #[test]
    fn reset_times_become_unix_seconds() {
        // 2026-10-20T12:00:00Z, checked with `date -u -d`
        assert_eq!(
            reset_seconds("2026-10-20T12:00:00+00:00"),
            Some(1_792_497_600)
        );
        // The same instant written with an offset is the same instant.
        assert_eq!(
            reset_seconds("2026-10-20T09:00:00-03:00"),
            reset_seconds("2026-10-20T12:00:00+00:00")
        );
        assert_eq!(reset_seconds("tomorrow"), None);
    }

    #[test]
    fn an_account_without_plan_limits_shows_nothing() {
        let api_key = body(r#"{ "rate_limits_available": false, "rate_limits": null }"#);
        assert!(parse_usage(&api_key, 100).is_none());
    }

    #[test]
    fn a_missing_window_is_skipped_and_none_at_all_is_unavailable() {
        let only_weekly = body(
            r#"{ "rate_limits_available": true, "rate_limits": { "five_hour": null,
                "seven_day": { "utilization": 40, "resets_at": "2026-10-26T20:00:00+00:00" } } }"#,
        );
        let usage = parse_usage(&only_weekly, 100).expect("a plan");
        assert_eq!(usage.windows.len(), 1);
        assert_eq!(usage.windows[0].kind, WindowKind::Weekly);

        let empty = body(r#"{ "rate_limits_available": true, "rate_limits": {} }"#);
        assert_eq!(
            parse_usage(&empty, 100).expect("a plan").state,
            PlanUsageState::Unavailable
        );
    }

    /// Writes a script that plays Claude Code: answers the `get_usage` control request.
    #[cfg(unix)]
    fn fake_claude(dir: &std::path::Path, body: &str) -> ProviderCommand {
        let script = dir.join("fake-claude.sh");
        fs::write(&script, body).expect("write script");
        ProviderCommand::new("sh", &[script.to_str().expect("utf8 path")])
    }

    #[cfg(unix)]
    #[test]
    fn the_control_request_gets_the_windows_back() {
        let dir = tempfile::tempdir().expect("dir");
        let command = fake_claude(
            dir.path(),
            r#"read -r _request
printf '%s\n' '{"type":"system","subtype":"hook_started"}'
printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"lynx-plan-usage","response":{"rate_limits_available":true,"subscription_type":"pro","rate_limits":{"five_hour":{"utilization":21,"resets_at":"2026-10-20T12:00:00+00:00"},"seven_day":null}}}}'
"#,
        );

        let usage = read(&command, Duration::from_secs(10)).expect("installed");

        assert_eq!(usage.state, PlanUsageState::Ready);
        assert_eq!(usage.windows[0].used_percent, 21);
    }

    #[cfg(unix)]
    #[test]
    fn a_not_signed_in_error_asks_the_user_to_sign_in() {
        let dir = tempfile::tempdir().expect("dir");
        let command = fake_claude(
            dir.path(),
            r#"read -r _request
printf '%s\n' '{"type":"control_response","response":{"subtype":"error","request_id":"lynx-plan-usage","error":"Not logged in · Please run /login"}}'
"#,
        );

        let usage = read(&command, Duration::from_secs(10)).expect("installed");

        assert_eq!(usage.state, PlanUsageState::SignInNeeded);
    }

    #[test]
    fn the_probe_settings_turn_hooks_off_and_are_written_where_the_command_points() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir
            .path()
            .join("lynx-usage")
            .join("claude-probe-settings.json");

        write_probe_settings(&path).expect("write");

        let written: Value =
            serde_json::from_str(&fs::read_to_string(&path).expect("read")).expect("json");
        assert_eq!(written["disableAllHooks"], true);
    }

    #[test]
    fn a_missing_claude_shows_nothing_at_all() {
        let command = ProviderCommand::new("lynx-no-such-claude-cli", &[]);
        assert!(read(&command, Duration::from_secs(1)).is_none());
    }

    /// Needs Claude Code on PATH, signed in with a plan. Run with `--ignored` to check the real CLI.
    #[test]
    #[ignore = "needs an installed, signed-in Claude Code CLI with a plan"]
    fn the_installed_claude_answers_with_its_limits() {
        let usage = read(&installed(), Duration::from_secs(30)).expect("installed");

        assert_eq!(usage.state, PlanUsageState::Ready, "{:?}", usage.reason);
        assert!(!usage.windows.is_empty());
    }
}
