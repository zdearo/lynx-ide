//! Reads the Codex plan limits from `codex app-server`, the protocol Codex's own clients speak.
//! Lynx never reads Codex's login token: the server answers for whichever account Codex is signed in with.

use std::process::Child;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::agents::harness::HarnessId;

use super::plan_usage::{PlanUsage, PlanWindow, WindowKind, now_seconds, whole_percent};
use super::provider_protocol::{ProviderCommand, lines_of, send, stop, take_input, wait_for};

const PROVIDER: &str = "Codex";
const INITIALIZE_ID: u64 = 1;
const READ_LIMITS_ID: u64 = 2;

/// The `app-server` command of the installed Codex CLI.
pub fn installed() -> ProviderCommand {
    ProviderCommand::new("codex", &["app-server"])
}

/// The plan limits Codex reports, or `None` when Codex is not installed, so there is nothing to show.
pub fn read(command: &ProviderCommand, timeout: Duration) -> Option<PlanUsage> {
    if !command.is_installed() {
        return None;
    }
    let observed_at = now_seconds();
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return Some(PlanUsage::from_failure(
                HarnessId::Codex,
                error.to_string(),
                observed_at,
            ));
        }
    };
    let outcome = exchange(&mut child, timeout);
    stop(&mut child);

    Some(match outcome {
        Ok(result) => parse_limits(&result, observed_at),
        Err(reason) => PlanUsage::from_failure(HarnessId::Codex, reason, observed_at),
    })
}

/// Initializes the connection, then asks for the limits. Returns the answer's `result`.
fn exchange(child: &mut Child, timeout: Duration) -> Result<Value, String> {
    let deadline = Instant::now() + timeout;
    let mut stdin = take_input(child)?;
    let lines = lines_of(child)?;

    send(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": INITIALIZE_ID,
            "method": "initialize",
            "params": { "clientInfo": { "name": "lynx", "version": env!("CARGO_PKG_VERSION") } }
        }),
    )?;
    wait_for(&lines, deadline, |message| answers(message, INITIALIZE_ID))
        .map_err(|silence| silence.describe(PROVIDER))?;

    send(
        &mut stdin,
        &json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }),
    )?;
    send(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": READ_LIMITS_ID,
            "method": "account/rateLimits/read",
            "params": {}
        }),
    )?;
    let answer = wait_for(&lines, deadline, |message| answers(message, READ_LIMITS_ID))
        .map_err(|silence| silence.describe(PROVIDER))?;

    if let Some(error) = answer.get("error") {
        return Err(error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("Codex returned an error")
            .to_owned());
    }
    answer
        .get("result")
        .cloned()
        .ok_or_else(|| "Codex sent an answer without a result".to_owned())
}

fn answers(message: &Value, id: u64) -> bool {
    message.get("id").and_then(Value::as_u64) == Some(id)
}

/// Turns the `account/rateLimits/read` result into what the status bar shows.
pub fn parse_limits(result: &Value, observed_at: u32) -> PlanUsage {
    let limits = &result["rateLimits"];
    if limits.is_null() {
        return PlanUsage::unavailable(
            HarnessId::Codex,
            "Codex did not report plan limits for this account".to_owned(),
            observed_at,
        );
    }
    let windows: Vec<PlanWindow> = ["primary", "secondary"]
        .iter()
        .filter_map(|key| window_of(&limits[*key]))
        .collect();
    if windows.is_empty() {
        return PlanUsage::unavailable(
            HarnessId::Codex,
            "Codex reported no limit windows".to_owned(),
            observed_at,
        );
    }
    let plan = limits["planType"].as_str().map(str::to_owned);
    PlanUsage::ready(HarnessId::Codex, plan, windows, observed_at)
}

fn window_of(value: &Value) -> Option<PlanWindow> {
    let used_percent = whole_percent(value["usedPercent"].as_f64()?);
    let minutes = value["windowDurationMins"]
        .as_u64()
        .and_then(|minutes| u32::try_from(minutes).ok());
    let resets_at = value["resetsAt"]
        .as_u64()
        .and_then(|seconds| u32::try_from(seconds).ok());
    Some(PlanWindow {
        kind: WindowKind::of_minutes(minutes),
        minutes,
        used_percent,
        resets_at,
    })
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::fs;

    use super::super::plan_usage::PlanUsageState;
    use super::*;

    /// What `codex app-server` answered for a free ChatGPT account, with the parts Lynx does not read cut.
    const FREE_ACCOUNT: &str = r#"{
        "rateLimits": {
            "limitId": "codex",
            "primary": { "usedPercent": 0, "windowDurationMins": 43200, "resetsAt": 1795073731 },
            "secondary": null,
            "planType": "free",
            "rateLimitReachedType": null
        }
    }"#;

    fn parsed(text: &str) -> PlanUsage {
        parse_limits(&serde_json::from_str(text).expect("json"), 100)
    }

    #[test]
    fn a_free_account_shows_its_one_monthly_window() {
        let usage = parsed(FREE_ACCOUNT);

        assert_eq!(usage.state, PlanUsageState::Ready);
        assert_eq!(usage.plan.as_deref(), Some("free"));
        assert_eq!(
            usage.windows,
            [PlanWindow {
                kind: WindowKind::Monthly,
                minutes: Some(43_200),
                used_percent: 0,
                resets_at: Some(1_795_073_731),
            }]
        );
    }

    #[test]
    fn primary_and_secondary_windows_are_both_kept_in_order() {
        let usage = parsed(
            r#"{ "rateLimits": {
                "primary": { "usedPercent": 42.5, "windowDurationMins": 300, "resetsAt": 10 },
                "secondary": { "usedPercent": 91, "windowDurationMins": 10080, "resetsAt": 20 },
                "planType": "plus"
            } }"#,
        );

        let kinds: Vec<WindowKind> = usage.windows.iter().map(|window| window.kind).collect();
        assert_eq!(kinds, [WindowKind::Session, WindowKind::Weekly]);
        assert_eq!(usage.windows[1].used_percent, 91);
        assert_eq!(usage.windows[0].used_percent, 43);
    }

    #[test]
    fn no_limits_for_the_account_is_unavailable_not_a_zero() {
        let usage = parsed(r#"{ "rateLimits": null }"#);

        assert_eq!(usage.state, PlanUsageState::Unavailable);
        assert!(usage.windows.is_empty());
        assert!(usage.reason.is_some());
    }

    /// Writes a script that plays the server: answers `initialize`, then the limits request.
    #[cfg(unix)]
    fn fake_server(dir: &std::path::Path, body: &str) -> ProviderCommand {
        let script = dir.join("fake-codex.sh");
        fs::write(&script, body).expect("write script");
        ProviderCommand::new("sh", &[script.to_str().expect("utf8 path")])
    }

    #[cfg(unix)]
    #[test]
    fn the_handshake_then_the_limits_read_gives_the_windows() {
        let dir = tempfile::tempdir().expect("dir");
        let command = fake_server(
            dir.path(),
            r#"read -r _init
printf '%s\n' '{"method":"account/updated","params":{}}'
printf '%s\n' '{"id":1,"result":{"userAgent":"fake"}}'
read -r _initialized
read -r _request
printf '%s\n' '{"id":2,"result":{"rateLimits":{"primary":{"usedPercent":12,"windowDurationMins":300,"resetsAt":99},"planType":"plus"}}}'
"#,
        );

        let usage = read(&command, Duration::from_secs(10)).expect("installed");

        assert_eq!(usage.state, PlanUsageState::Ready);
        assert_eq!(usage.plan.as_deref(), Some("plus"));
        assert_eq!(usage.windows[0].used_percent, 12);
    }

    #[cfg(unix)]
    #[test]
    fn a_server_that_never_answers_is_given_up_on_at_the_deadline() {
        let dir = tempfile::tempdir().expect("dir");
        let command = fake_server(dir.path(), "read -r _init\nsleep 30\n");

        let started = Instant::now();
        let usage = read(&command, Duration::from_millis(300)).expect("installed");

        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(usage.state, PlanUsageState::Unavailable);
        assert_eq!(
            usage.reason.as_deref(),
            Some("Codex did not answer in time")
        );
    }

    #[test]
    fn a_missing_codex_shows_nothing_at_all() {
        let command = ProviderCommand::new("lynx-no-such-codex-cli", &["app-server"]);
        assert!(read(&command, Duration::from_secs(1)).is_none());
    }

    /// Needs a Codex CLI on PATH and signed in, so CI skips it. Run with `--ignored` to check the real server.
    #[test]
    #[ignore = "needs an installed, signed-in Codex CLI"]
    fn the_installed_codex_answers_with_its_limits() {
        let usage = read(&installed(), Duration::from_secs(20)).expect("installed");

        assert_eq!(usage.state, PlanUsageState::Ready, "{:?}", usage.reason);
        assert!(!usage.windows.is_empty());
    }
}
