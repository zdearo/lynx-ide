//! `lynx-ide --cli`: the tools the MCP server offers, run from a shell. An agent calls them with its Bash
//! tool, and its skill says how. The app answers over the same loopback route as the MCP server, so the
//! session the terminal received identifies the caller. A bot's Bash gets the same session in its env.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use base64::Engine;
use serde_json::Value;

use super::tool_catalog::list_for;
use super::tool_transport::{LoopbackTransport, ToolTransport};

pub const FLAG: &str = "--cli";

const USAGE: &str = "usage: <lynx-ide> --cli list | <tool> [arguments as JSON, or - to read them from stdin] | help";

/// Runs the CLI when the arguments ask for it and returns the exit code. None means the app starts normally.
pub fn run_if_requested(arguments: &[String]) -> Option<i32> {
    if arguments.get(1).map(String::as_str) != Some(FLAG) {
        return None;
    }
    let transport = LoopbackTransport::from_env(None);
    let image_dir = std::env::temp_dir().join("lynx-images");
    let code = run(
        &arguments[2..],
        &transport,
        std::io::stdin().lock(),
        &image_dir,
        &mut std::io::stdout().lock(),
        &mut std::io::stderr().lock(),
    );
    Some(code)
}

/// Runs one CLI command. Text answers go to `out`, failures and the reason go to `err`. Images are written to
/// `image_dir`, and their path is printed, so the agent can open them with its file tools.
pub fn run(
    args: &[String],
    transport: &impl ToolTransport,
    mut input: impl Read,
    image_dir: &Path,
    out: &mut impl Write,
    err: &mut impl Write,
) -> i32 {
    match args.first().map(String::as_str) {
        None | Some("help") => {
            let _ = writeln!(out, "{USAGE}");
            0
        }
        Some("list") => match serde_json::to_string_pretty(&list_for(transport.caller())) {
            Ok(text) => {
                let _ = writeln!(out, "{text}");
                0
            }
            Err(error) => {
                let _ = writeln!(err, "could not list the tools: {error}");
                1
            }
        },
        Some(tool) => {
            let text = match args.get(1).map(String::as_str) {
                None => "{}".to_owned(),
                Some("-") => {
                    let mut text = String::new();
                    if let Err(error) = input.read_to_string(&mut text) {
                        let _ = writeln!(err, "could not read the arguments from stdin: {error}");
                        return 2;
                    }
                    text
                }
                Some(json) => json.to_owned(),
            };
            let arguments: Value = match serde_json::from_str(&text) {
                Ok(arguments) => arguments,
                Err(error) => {
                    let _ = writeln!(err, "the arguments are not JSON: {error}");
                    return 2;
                }
            };
            match transport.call(tool, &arguments) {
                Ok(result) => print_result(&result, image_dir, out, err),
                Err(reason) => {
                    let _ = writeln!(err, "{reason}");
                    1
                }
            }
        }
    }
}

/// Prints the content of an MCP tool result: text lines, and images saved as files.
fn print_result(
    result: &Value,
    image_dir: &Path,
    out: &mut impl Write,
    err: &mut impl Write,
) -> i32 {
    let failed = result
        .get("isError")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut code = i32::from(failed);
    for item in result
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        match item.get("type").and_then(Value::as_str) {
            Some("text") => {
                let text = item.get("text").and_then(Value::as_str).unwrap_or_default();
                if failed {
                    let _ = writeln!(err, "{text}");
                } else {
                    let _ = writeln!(out, "{text}");
                }
            }
            Some("image") => match save_image(item, image_dir) {
                Ok(path) => {
                    let _ = writeln!(out, "image saved: {}", path.display());
                }
                Err(reason) => {
                    let _ = writeln!(err, "{reason}");
                    code = 1;
                }
            },
            _ => {}
        }
    }
    code
}

/// Writes one image of a result to `image_dir` and returns its path.
fn save_image(item: &Value, image_dir: &Path) -> Result<PathBuf, String> {
    let data = item
        .get("data")
        .and_then(Value::as_str)
        .ok_or("an image came without its data")?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|error| format!("could not decode an image: {error}"))?;
    let extension = match item.get("mimeType").and_then(Value::as_str) {
        Some("image/jpeg") => "jpg",
        Some("image/webp") => "webp",
        _ => "png",
    };
    fs::create_dir_all(image_dir)
        .map_err(|error| format!("could not create {}: {error}", image_dir.display()))?;
    let path = image_dir.join(format!("{}.{extension}", uuid::Uuid::new_v4()));
    fs::write(&path, bytes).map_err(|error| format!("could not save the image: {error}"))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A transport that answers with `result` and remembers each call.
    struct Canned {
        result: Result<Value, String>,
        caller: Option<String>,
        calls: std::sync::Mutex<Vec<(String, Value)>>,
    }

    impl Canned {
        fn answering(result: Result<Value, String>) -> Self {
            Self {
                result,
                caller: None,
                calls: std::sync::Mutex::new(Vec::new()),
            }
        }
    }

    impl ToolTransport for Canned {
        fn call(&self, tool: &str, arguments: &Value) -> Result<Value, String> {
            self.calls
                .lock()
                .expect("lock")
                .push((tool.to_owned(), arguments.clone()));
            self.result.clone()
        }

        fn caller(&self) -> Option<&str> {
            self.caller.as_deref()
        }
    }

    fn argv(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|part| (*part).to_owned()).collect()
    }

    /// Runs the CLI with `args` and returns (code, stdout, stderr).
    fn cli(transport: &Canned, args: &[&str], stdin: &str) -> (i32, String, String) {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = run(
            &argv(args),
            transport,
            stdin.as_bytes(),
            &dir.path().join("images"),
            &mut out,
            &mut err,
        );
        (
            code,
            String::from_utf8(out).expect("utf8"),
            String::from_utf8(err).expect("utf8"),
        )
    }

    #[test]
    fn no_arguments_and_help_print_the_usage() {
        let transport = Canned::answering(Ok(json!({})));
        let (code, out, _) = cli(&transport, &[], "");
        assert_eq!(code, 0);
        assert!(out.starts_with("usage:"));
        assert_eq!(cli(&transport, &["help"], "").0, 0);
    }

    #[test]
    fn a_tool_name_is_called_with_its_json_arguments() {
        let transport = Canned::answering(Ok(
            json!({ "content": [{ "type": "text", "text": "started" }], "isError": false }),
        ));
        let (code, out, _) = cli(&transport, &["lynx_worker_start", r#"{"task":"t1"}"#], "");
        assert_eq!(code, 0);
        assert_eq!(out, "started\n");
        let calls = transport.calls.lock().expect("lock");
        assert_eq!(calls[0].0, "lynx_worker_start");
        assert_eq!(calls[0].1, json!({ "task": "t1" }));
    }

    #[test]
    fn the_arguments_can_come_from_stdin_and_default_to_none() {
        let transport = Canned::answering(Ok(json!({ "content": [] })));
        assert_eq!(
            cli(&transport, &["lynx_check", "-"], r#"{"waitSeconds":0}"#).0,
            0
        );
        assert_eq!(
            transport.calls.lock().expect("lock")[0].1,
            json!({ "waitSeconds": 0 })
        );
        assert_eq!(cli(&transport, &["lynx_check"], "").0, 0);
        assert_eq!(transport.calls.lock().expect("lock")[1].1, json!({}));
    }

    #[test]
    fn arguments_that_are_not_json_are_refused_before_any_call() {
        let transport = Canned::answering(Ok(json!({})));
        let (code, _, err) = cli(&transport, &["lynx_check", "{oops"], "");
        assert_eq!(code, 2);
        assert!(err.contains("not JSON"));
        assert!(transport.calls.lock().expect("lock").is_empty());
    }

    #[test]
    fn a_tool_error_goes_to_stderr_and_fails_the_command() {
        let transport = Canned::answering(Ok(json!({
            "content": [{ "type": "text", "text": "no run yet" }],
            "isError": true
        })));
        let (code, out, err) = cli(&transport, &["lynx_worker_start"], "");
        assert_eq!(code, 1);
        assert_eq!(out, "");
        assert_eq!(err, "no run yet\n");
    }

    #[test]
    fn a_transport_failure_says_why_and_fails() {
        let transport = Canned::answering(Err(
            "Could not reach Lynx. Check that the app is still open.".to_owned(),
        ));
        let (code, _, err) = cli(&transport, &["lynx_check"], "");
        assert_eq!(code, 1);
        assert!(err.contains("Could not reach Lynx"));
    }

    #[test]
    fn an_image_is_saved_as_a_file_and_its_path_is_printed() {
        let png = [0x89, b'P', b'N', b'G', 1, 2, 3];
        let encoded = base64::engine::general_purpose::STANDARD.encode(png);
        let transport = Canned::answering(Ok(json!({
            "content": [{ "type": "image", "data": encoded, "mimeType": "image/png" }],
            "isError": false
        })));
        let dir = tempfile::tempdir().expect("temp dir");
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = run(
            &argv(&["browser_screenshot"]),
            &transport,
            "".as_bytes(),
            &dir.path().join("images"),
            &mut out,
            &mut err,
        );
        assert_eq!(code, 0);
        let line = String::from_utf8(out).expect("utf8");
        let path = line
            .trim()
            .strip_prefix("image saved: ")
            .expect("a saved image line");
        assert!(path.ends_with(".png"));
        assert_eq!(fs::read(path).expect("file"), png);
    }

    #[test]
    fn list_shows_the_tools_the_caller_has() {
        let transport = Canned::answering(Ok(json!({})));
        let (code, out, _) = cli(&transport, &["list"], "");
        assert_eq!(code, 0);
        assert!(out.contains("lynx_worker_start"), "{out}");
        assert!(
            out.contains("browser_"),
            "the browser tools are listed too: {out}"
        );
    }

    #[test]
    fn a_bot_lists_its_own_tools_and_the_orchestration_but_no_browser() {
        let transport = Canned {
            result: Ok(json!({})),
            caller: Some("bot:ada:t1".to_owned()),
            calls: std::sync::Mutex::new(Vec::new()),
        };
        let (_, out, _) = cli(&transport, &["list"], "");
        let names: Vec<String> = serde_json::from_str::<Vec<Value>>(&out)
            .expect("the list is JSON")
            .iter()
            .filter_map(|tool| tool["name"].as_str().map(str::to_owned))
            .collect();
        // A bot orchestrates, so it has the lynx_* orchestration tools too; it has no browser tools.
        assert!(names.contains(&"bot_request".to_owned()), "{names:?}");
        assert!(names.contains(&"lynx_worker_start".to_owned()), "{names:?}");
        assert!(
            names.iter().all(|name| !name.starts_with("browser_")),
            "{names:?}"
        );
    }
}
