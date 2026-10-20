//! The Lynx side of the link: starts the host process, accepts its connection, and speaks the
//! protocol. Everything about one running host lives in a [`HostLink`]; dropping it stops the host.

use std::io::{self, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::frame_buffer::{FrameBuffer, FrameError};
use crate::message::{FromHost, ToHost, read_message, write_message};
use crate::transport::{Connection, Listener};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
const STOP_TIMEOUT: Duration = Duration::from_secs(3);

/// What the host side did, as seen from Lynx.
#[derive(Debug, Clone, PartialEq)]
pub enum LinkEvent {
    Message(FromHost),
    /// The connection ended: the host exited, crashed or closed it.
    Closed,
}

#[derive(Debug, thiserror::Error)]
pub enum LinkError {
    #[error("the browser host could not be started: {0}")]
    Start(io::Error),
    #[error("the browser host did not connect within {} seconds", CONNECT_TIMEOUT.as_secs())]
    ConnectTimeout,
    #[error("the browser host exited before it connected ({0})")]
    ExitedEarly(String),
    #[error("link to the browser host: {0}")]
    Io(#[from] io::Error),
    #[error(transparent)]
    Frames(#[from] FrameError),
    #[error("the link to the browser host is closed")]
    Closed,
}

pub struct HostLink {
    writer: Mutex<Connection>,
    frames: Mutex<FrameBuffer>,
    child: Mutex<Child>,
    dir: PathBuf,
    closed: Arc<AtomicBool>,
}

/// Frames are rewritten many times a second, so they live in RAM where there is a RAM-backed
/// folder, not in a file the system would write to disk. (Elsewhere the temp folder is as close
/// as it gets; the frame file asks not to be written out.)
fn runtime_root() -> PathBuf {
    let shared = Path::new("/dev/shm");
    if cfg!(target_os = "linux") && shared.is_dir() {
        shared.to_path_buf()
    } else {
        std::env::temp_dir()
    }
}

fn new_session_dir() -> io::Result<PathBuf> {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let dir = runtime_root().join(format!(
        "lynx-browser-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    // Private to this user: the socket in it can drive the browser.
    #[cfg_attr(windows, allow(unused_mut))]
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&dir)?;
    Ok(dir)
}

impl HostLink {
    /// Starts `host`, waits for it to connect, and returns once the link is up. `on_event` runs on
    /// a thread of its own for every message, and once more when the link closes; it must not
    /// block.
    pub fn spawn(
        host: &Path,
        on_event: impl Fn(LinkEvent) + Send + 'static,
    ) -> Result<Self, LinkError> {
        let dir = new_session_dir()?;
        match Self::start(host, &dir, on_event) {
            Ok(link) => Ok(link),
            Err(error) => {
                let _ = std::fs::remove_dir_all(&dir);
                Err(error)
            }
        }
    }

    fn start(
        host: &Path,
        dir: &Path,
        on_event: impl Fn(LinkEvent) + Send + 'static,
    ) -> Result<Self, LinkError> {
        let frames_path = dir.join("frames");
        let frames = FrameBuffer::create(&frames_path)?;
        let listener = Listener::bind(dir)?;

        let mut command = Command::new(host);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // CREATE_NO_WINDOW: the host has no console to show.
            command.creation_flags(0x0800_0000);
        }
        let mut child = command
            .arg("--socket")
            .arg(listener.address())
            .arg("--frames")
            .arg(&frames_path)
            .arg("--cache")
            .arg(dir.join("cache"))
            .stdin(Stdio::null())
            .spawn()
            .map_err(LinkError::Start)?;

        let stream = match Self::accept(&listener, &mut child) {
            Ok(stream) => stream,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };

        let closed = Arc::new(AtomicBool::new(false));
        let reader = BufReader::new(stream.clone());
        let reader_closed = Arc::clone(&closed);
        thread::Builder::new()
            .name("browser-link".into())
            .spawn(move || {
                let mut reader = reader;
                while let Ok(Some(message)) = read_message::<FromHost>(&mut reader) {
                    on_event(LinkEvent::Message(message));
                }
                reader_closed.store(true, Ordering::Release);
                on_event(LinkEvent::Closed);
            })?;

        Ok(Self {
            writer: Mutex::new(stream),
            frames: Mutex::new(frames),
            child: Mutex::new(child),
            dir: dir.to_path_buf(),
            closed,
        })
    }

    fn accept(listener: &Listener, child: &mut Child) -> Result<Connection, LinkError> {
        let started = Instant::now();
        loop {
            if let Some(connection) = listener.try_accept()? {
                return Ok(connection);
            }
            if let Some(status) = child.try_wait()? {
                return Err(LinkError::ExitedEarly(status.to_string()));
            }
            if started.elapsed() > CONNECT_TIMEOUT {
                return Err(LinkError::ConnectTimeout);
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    pub fn send(&self, message: &ToHost) -> Result<(), LinkError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(LinkError::Closed);
        }
        let mut writer = self
            .writer
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        write_message(&mut *writer, message).map_err(LinkError::Io)
    }

    /// Copies a frame out of the shared memory into `out` (which is cleared first), so the slot
    /// can be handed back to the host right away.
    pub fn copy_frame(
        &self,
        slot: u8,
        width: u32,
        height: u32,
        out: &mut Vec<u8>,
    ) -> Result<(), LinkError> {
        let length = width as usize * height as usize * 4;
        let frames = self
            .frames
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let pixels = frames.read(usize::from(slot), length)?;
        out.clear();
        out.extend_from_slice(pixels);
        Ok(())
    }

    /// The host's process id, for diagnostics and measurements.
    pub fn pid(&self) -> u32 {
        self.child
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .id()
    }

    pub fn is_alive(&self) -> bool {
        if self.closed.load(Ordering::Acquire) {
            return false;
        }
        let mut child = self
            .child
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        matches!(child.try_wait(), Ok(None))
    }

    /// Asks the host to quit, and kills it if it does not. Safe to call more than once.
    pub fn shutdown(&self) {
        let _ = self.send(&ToHost::Shutdown);
        let mut child = self
            .child
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let started = Instant::now();
        while started.elapsed() < STOP_TIMEOUT {
            if matches!(child.try_wait(), Ok(Some(_))) {
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }
        let _ = child.kill();
        let _ = child.wait();
    }
}

impl Drop for HostLink {
    fn drop(&mut self) {
        self.shutdown();
        // The frame file must be let go of first, or Windows will not delete it.
        self.frames
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .unmap();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use super::*;

    #[cfg(unix)]
    // One test for both failures: they count the session folders of this process, so they must
    // not run side by side.
    #[test]
    fn a_host_that_cannot_start_is_reported_and_leaves_nothing_behind() {
        let before = count_session_dirs();

        let error = HostLink::spawn(Path::new("/nonexistent/lynx-browser-host"), |_| {})
            .err()
            .expect("there is no such program");
        assert!(matches!(error, LinkError::Start(_)), "{error}");

        let started = Instant::now();
        let error = HostLink::spawn(Path::new("/bin/true"), |_| {})
            .err()
            .expect("the program never connects");
        assert!(matches!(error, LinkError::ExitedEarly(_)), "{error}");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "an exited host is reported, not waited for"
        );

        assert_eq!(count_session_dirs(), before);
    }

    #[cfg(unix)]
    fn count_session_dirs() -> usize {
        let prefix = format!("lynx-browser-{}-", std::process::id());
        std::fs::read_dir(runtime_root())
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|entry| entry.file_name().to_string_lossy().starts_with(&prefix))
                    .count()
            })
            .unwrap_or(0)
    }
}
