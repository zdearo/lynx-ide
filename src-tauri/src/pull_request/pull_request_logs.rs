//! The log of a GitHub Actions job, for a check that failed or that someone wants to read.

use std::path::Path;
use std::time::Duration;

use serde::Serialize;
use specta::Type;

use super::github_remote::{GitHubRemoteError, run_gh};

const TIMEOUT: Duration = Duration::from_secs(90);
/// A log can run to hundreds of thousands of lines; the end is where a failure is.
const MAX_LINES: usize = 400;
const MAX_BYTES: usize = 200_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CheckLog {
    pub text: String,
    /// The start of the log was cut: only its end is shown.
    pub truncated: bool,
}

/// The job id of a GitHub Actions page (`.../actions/runs/<run>/job/<job>`), digits only.
pub fn parse_job_id(details_url: &str) -> Option<u64> {
    if !details_url.starts_with("https://") || !details_url.contains("/actions/runs/") {
        return None;
    }
    let after = details_url.split("/job/").nth(1)?;
    let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// Removes ANSI colour codes, which a log viewer would show as noise.
fn strip_ansi(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            for next in chars.by_ref() {
                if next.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// `job<TAB>step<TAB>timestamp message` lines become a step heading and the messages under it,
/// without the timestamp, the colour codes or the group markers.
pub fn format_log(raw: &str) -> String {
    let mut out = String::new();
    let mut current_step = String::new();
    for line in raw.lines() {
        let mut parts = line.splitn(3, '\t');
        let (Some(_job), Some(step), Some(rest)) = (parts.next(), parts.next(), parts.next())
        else {
            out.push_str(&strip_ansi(line));
            out.push('\n');
            continue;
        };
        if step != current_step {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&format!("## {step}\n"));
            current_step = step.to_owned();
        }
        let rest = rest.trim_start_matches('\u{feff}');
        // The timestamp is the first word: `2026-10-19T16:43:48.9745191Z`.
        let message = rest.split_once(' ').map_or(rest, |(_, message)| message);
        let message = strip_ansi(message);
        if message.starts_with("##[endgroup]") {
            continue;
        }
        let message = message.replacen("##[group]", "› ", 1);
        out.push_str(&message);
        out.push('\n');
    }
    out
}

/// Keeps the last `MAX_LINES` lines (and at most `MAX_BYTES`), saying whether anything was cut.
pub fn tail(text: &str) -> (String, bool) {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(MAX_LINES);
    let mut kept = lines[start..].join("\n");
    let mut cut = start > 0;
    if kept.len() > MAX_BYTES {
        let mut from = kept.len() - MAX_BYTES;
        while !kept.is_char_boundary(from) {
            from += 1;
        }
        kept = kept[from..].to_owned();
        cut = true;
    }
    (kept, cut)
}

/// The log of the job a check's page points at. `failed_only` asks for just the failed steps.
pub fn read(
    root: &Path,
    details_url: &str,
    failed_only: bool,
) -> Result<CheckLog, GitHubRemoteError> {
    let Some(job) = parse_job_id(details_url) else {
        return Err(GitHubRemoteError::Failed(
            "this check has no GitHub Actions log".to_owned(),
        ));
    };
    let flag = if failed_only { "--log-failed" } else { "--log" };
    let raw = run_gh(
        &["run", "view", "--job", &job.to_string(), flag],
        Some(root),
        TIMEOUT,
    )?;
    let (text, truncated) = tail(&format_log(&raw));
    Ok(CheckLog { text, truncated })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_actions_job_pages_give_a_job_id() {
        assert_eq!(
            parse_job_id("https://github.com/acme/app/actions/runs/39/job/118180250170?pr=1"),
            Some(118_180_250_170)
        );
        assert_eq!(
            parse_job_id("https://github.com/acme/app/actions/runs/39"),
            None
        );
        assert_eq!(parse_job_id("https://ci.example.com/build/7"), None);
        assert_eq!(
            parse_job_id("http://github.com/a/b/actions/runs/1/job/2"),
            None
        );
        assert_eq!(
            parse_job_id("https://github.com/a/b/actions/runs/1/job/x"),
            None
        );
    }

    #[test]
    fn a_log_is_grouped_by_step_without_timestamps_or_colour() {
        let raw = "clean\tSet up job\t\u{feff}2026-10-19T16:43:48Z Current runner version\n\
                   clean\tSet up job\t2026-10-19T16:43:49Z ##[group]Permissions\n\
                   clean\tSet up job\t2026-10-19T16:43:49Z ##[endgroup]\n\
                   clean\tRun tests\t2026-10-19T16:44:00Z \u{1b}[31merror\u{1b}[0m: it broke\n";

        let formatted = format_log(raw);

        assert_eq!(
            formatted,
            "## Set up job\nCurrent runner version\n› Permissions\n\n## Run tests\nerror: it broke\n"
        );
    }

    #[test]
    fn only_the_end_of_a_long_log_is_kept() {
        let text = (1..=1000)
            .map(|n| format!("line {n}"))
            .collect::<Vec<_>>()
            .join("\n");

        let (kept, cut) = tail(&text);

        assert!(cut);
        assert_eq!(kept.lines().count(), MAX_LINES);
        assert!(kept.ends_with("line 1000"));
        assert!(!tail("short").1);
    }
}
