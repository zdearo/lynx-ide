use std::sync::Arc;

use tauri::State;
use tauri::ipc::Channel;

use crate::error::{AppError, IpcError};
use crate::process::blocking_task::run_blocking;

use super::daemon_host::DaemonStatus;
use super::output_channels::{OutputBytes, OutputChannels};
use super::terminal_manager::{ResumableSession, TerminalManager};
use super::terminal_session::{TerminalInfo, TerminalSnapshot};

#[tauri::command]
#[specta::specta]
pub async fn create_terminal(
    manager: State<'_, TerminalManager>,
    worktree_path: String,
    command: Option<String>,
    cols: u16,
    rows: u16,
) -> Result<TerminalInfo, IpcError> {
    let manager = manager.inner().clone();
    run_blocking(move || Ok(manager.create(&worktree_path, command.as_deref(), cols, rows)?)).await
}

#[tauri::command]
#[specta::specta]
pub async fn pty_daemon_status(
    manager: State<'_, TerminalManager>,
) -> Result<DaemonStatus, IpcError> {
    Ok(manager.daemon_status())
}

/// The memory the daemon's sessions hold, in MiB; None when the daemon is off or does not answer.
#[tauri::command]
#[specta::specta]
pub async fn pty_daemon_memory(
    manager: State<'_, TerminalManager>,
) -> Result<Option<u32>, IpcError> {
    let manager = manager.inner().clone();
    run_blocking(move || Ok(manager.daemon_memory_mib())).await
}

/// Freezes a terminal of the background process that the user is not using. Refused, with the
/// reason as the message, while it cannot sleep.
#[tauri::command]
#[specta::specta]
pub async fn hibernate_terminal(
    manager: State<'_, TerminalManager>,
    id: String,
) -> Result<(), IpcError> {
    let manager = manager.inner().clone();
    run_blocking(move || Ok(manager.hibernate(&id)?)).await
}

/// Takes an ended agent conversation off the resumable list. The agent keeps the conversation.
#[tauri::command]
#[specta::specta]
pub async fn forget_resumable_session(
    manager: State<'_, TerminalManager>,
    id: String,
) -> Result<(), IpcError> {
    let manager = manager.inner().clone();
    run_blocking(move || Ok(manager.forget_resumable(&id)?)).await
}

#[tauri::command]
#[specta::specta]
pub async fn list_resumable_sessions(
    manager: State<'_, TerminalManager>,
    worktree_path: String,
) -> Result<Vec<ResumableSession>, IpcError> {
    Ok(manager.resumable(&worktree_path))
}

/// Chooses what closing the app does with the daemon's sessions, before the window closes.
#[tauri::command]
#[specta::specta]
pub async fn prepare_quit(
    manager: State<'_, TerminalManager>,
    keep_sessions: bool,
) -> Result<(), IpcError> {
    manager.set_keep_on_exit(keep_sessions);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn list_terminals(
    manager: State<'_, TerminalManager>,
) -> Result<Vec<TerminalInfo>, IpcError> {
    Ok(manager.list())
}

/// Starts the output stream of a session to `on_output` and returns what it printed so far.
/// The channel is attached first, so a chunk printed meanwhile is in both and the view skips the
/// repeat by its `seq`; attached second, it could fall in the gap between them.
///
/// Not `async`: Tauri runs it in the order the page sent it. Two attaches in a row (a view that is
/// mounted twice) must end with the second one attached, which tasks racing on a pool would not promise.
#[tauri::command]
#[specta::specta]
pub fn terminal_attach(
    manager: State<'_, TerminalManager>,
    channels: State<'_, Arc<OutputChannels>>,
    id: String,
    on_output: Channel<OutputBytes>,
) -> Result<TerminalSnapshot, IpcError> {
    channels.attach(&id, on_output);
    manager.snapshot(&id).map_err(|error| {
        channels.detach(&id);
        AppError::from(error).into()
    })
}

#[tauri::command]
#[specta::specta]
pub async fn write_terminal(
    manager: State<'_, TerminalManager>,
    id: String,
    data: String,
) -> Result<(), IpcError> {
    let manager = manager.inner().clone();
    // Writes can block when the program is not reading, so they stay off the async runtime.
    run_blocking(move || Ok(manager.write(&id, &data)?)).await
}

#[tauri::command]
#[specta::specta]
pub async fn resize_terminal(
    manager: State<'_, TerminalManager>,
    id: String,
    cols: u16,
    rows: u16,
) -> Result<(), IpcError> {
    manager
        .resize(&id, cols, rows)
        .map_err(|error| AppError::from(error).into())
}

#[tauri::command]
#[specta::specta]
pub async fn mark_agent_seen(
    manager: State<'_, TerminalManager>,
    id: String,
) -> Result<(), IpcError> {
    manager
        .mark_seen(&id)
        .map_err(|error| AppError::from(error).into())
}

#[tauri::command]
#[specta::specta]
pub async fn close_terminal(
    manager: State<'_, TerminalManager>,
    channels: State<'_, Arc<OutputChannels>>,
    id: String,
) -> Result<(), IpcError> {
    channels.detach(&id);
    manager
        .close(&id)
        .map_err(|error| AppError::from(error).into())
}

#[cfg(all(test, unix))]
mod tests {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    use tauri::ipc::{CallbackFn, InvokeBody, InvokeResponseBody};
    use tauri::test::{INVOKE_KEY, get_ipc_response, mock_builder, mock_context, noop_assets};
    use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

    use crate::agents::hook_hub::HookHub;
    use crate::browser::SessionOwners;
    use crate::terminal::Shell;
    use crate::terminal::output_channels::OutputChannels;
    use crate::terminal::terminal_events::TerminalEventSink;

    use super::*;

    /// The frames the page would receive, with the index each was sent under.
    type Delivered = Arc<Mutex<Vec<(usize, Vec<u8>)>>>;

    /// What the app's sink does with output, without an app handle.
    struct ChannelSink(Arc<OutputChannels>);

    impl TerminalEventSink for ChannelSink {
        fn output(&self, id: &str, seq: u32, data: String) {
            self.0.send(id, seq, &data);
        }
        fn exit(&self, id: &str, _exit_code: Option<i32>) {
            self.0.detach(id);
        }
    }

    fn wait_until(what: &str, condition: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !condition() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// The command as the page calls it, through Tauri's own argument handling: the channel
    /// arrives as `__CHANNEL__:<id>`, and what Rust sends on it is read here as the page would get it.
    #[test]
    fn attaching_returns_the_snapshot_and_then_streams_raw_frames_in_order() {
        let channels = Arc::new(OutputChannels::default());
        let manager = TerminalManager::new(
            Arc::new(|| Shell {
                program: "/bin/sh".to_owned(),
                args: Vec::new(),
            }),
            Arc::new(ChannelSink(Arc::clone(&channels))),
            HookHub::disabled(),
            SessionOwners::default(),
            None,
            None,
        );
        let dir = tempfile::tempdir().expect("dir");
        let info = manager
            .create(
                &dir.path().to_string_lossy(),
                Some("echo first-$((1+1))"),
                80,
                24,
            )
            .expect("create");
        wait_until("the first output", || {
            manager
                .snapshot(&info.id)
                .is_ok_and(|s| s.data.contains("first-2"))
        });

        let delivered: Delivered = Arc::default();
        let store = Arc::clone(&delivered);
        let app = mock_builder()
            .manage(manager.clone())
            .manage(Arc::clone(&channels))
            .invoke_handler(tauri::generate_handler![terminal_attach])
            // Takes the place of the delivery to the page, so the frames can be read.
            .channel_interceptor(move |_, _, index, body| {
                if let InvokeResponseBody::Raw(bytes) = body {
                    store.lock().expect("lock").push((index, bytes.clone()));
                }
                true
            })
            .build(mock_context(noop_assets()))
            .expect("app");
        let webview = WebviewWindowBuilder::new(&app, "main", WebviewUrl::default())
            .build()
            .expect("window");

        let request = tauri::webview::InvokeRequest {
            cmd: "terminal_attach".into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "tauri://localhost".parse().expect("url"),
            body: InvokeBody::Json(serde_json::json!({
                "id": info.id,
                "onOutput": "__CHANNEL__:7",
            })),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.into(),
        };
        let response = get_ipc_response(&webview, request).expect("attach");
        let InvokeResponseBody::Json(json) = response else {
            panic!("the snapshot is JSON");
        };
        let snapshot: serde_json::Value = serde_json::from_str(&json).expect("json");
        assert!(snapshot["data"].as_str().expect("data").contains("first-2"));
        let last_seq = snapshot["lastSeq"].as_u64().expect("seq");
        assert!(last_seq >= 1);

        manager
            .write(&info.id, "echo second-$((2+2))\r")
            .expect("write");
        wait_until("the streamed output", || {
            delivered
                .lock()
                .expect("lock")
                .iter()
                .any(|(_, frame)| String::from_utf8_lossy(&frame[4..]).contains("second-4"))
        });

        let frames = delivered.lock().expect("lock");
        let indexes: Vec<usize> = frames.iter().map(|(index, _)| *index).collect();
        assert_eq!(
            indexes,
            (0..frames.len()).collect::<Vec<_>>(),
            "numbered from 0, in order"
        );
        let seqs: Vec<u32> = frames
            .iter()
            .map(|(_, frame)| u32::from_le_bytes(frame[..4].try_into().expect("four bytes")))
            .collect();
        assert!(
            seqs.iter().all(|seq| u64::from(*seq) > last_seq),
            "only what came after the snapshot: {seqs:?}"
        );
        assert!(
            seqs.windows(2).all(|pair| pair[0] < pair[1]),
            "increasing: {seqs:?}"
        );
        drop(frames);
        let _ = app.state::<TerminalManager>();
        manager.close_all();
    }

    #[test]
    fn attaching_to_a_session_that_does_not_exist_is_an_error_and_leaves_no_channel() {
        let channels = Arc::new(OutputChannels::default());
        let manager = TerminalManager::new(
            Arc::new(|| Shell {
                program: "/bin/sh".to_owned(),
                args: Vec::new(),
            }),
            Arc::new(ChannelSink(Arc::clone(&channels))),
            HookHub::disabled(),
            SessionOwners::default(),
            None,
            None,
        );
        let app = mock_builder()
            .manage(manager)
            .manage(Arc::clone(&channels))
            .invoke_handler(tauri::generate_handler![terminal_attach])
            .build(mock_context(noop_assets()))
            .expect("app");
        let webview = WebviewWindowBuilder::new(&app, "main", WebviewUrl::default())
            .build()
            .expect("window");

        let request = tauri::webview::InvokeRequest {
            cmd: "terminal_attach".into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "tauri://localhost".parse().expect("url"),
            body: InvokeBody::Json(serde_json::json!({
                "id": "missing",
                "onOutput": "__CHANNEL__:9",
            })),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.into(),
        };

        assert!(get_ipc_response(&webview, request).is_err());
        assert!(!channels.is_attached("missing"));
    }
}
