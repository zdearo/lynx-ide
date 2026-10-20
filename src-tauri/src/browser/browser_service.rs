//! Owns the browser engine process and what Lynx knows about it. The engine starts when the
//! browser is first opened and stops with the app; if it goes away on its own, the state says so
//! and the next open starts a new one.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, Weak};
use std::time::{Duration, Instant};

use serde_json::Value;

use lynx_browser_protocol::link::{HostLink, LinkEvent};
use lynx_browser_protocol::message::{FromHost, ToHost};

use super::agent_pointer::{AgentPointer, Controller, CursorShape, Sweep};
use super::browser_error::BrowserError;
use super::browser_state::{BrowserState, BrowserStatus};
use super::engine_files::{self, EngineStatus};
use super::engine_install_error::EngineInstallError;
use super::engine_installer::{EngineInstallLock, EngineProgress, install};
use super::engine_location::locate;
use super::engine_manifest::manifest_url;
use super::page_log::PageLog;
use super::web_address::normalize;

/// How long a freshly started engine has to say it is ready.
const READY_TIMEOUT: Duration = Duration::from_secs(20);

/// One frame of the page, BGRA, in device pixels.
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// Where Lynx tells the interface about changes.
pub trait BrowserEvents: Send + Sync + 'static {
    fn state_changed(&self, state: &BrowserState);

    /// No agent is in control of the page any more.
    fn agent_released(&self) {}
}

/// Where frames go to be shown. Called on the link's thread, so it must not block.
pub trait FrameSink: Send + Sync + 'static {
    fn frame(&self, frame: Frame);

    /// An agent's pointer or control changed, so the view must repaint and keep animating.
    fn pointer_changed(&self) {}
}

#[derive(Clone, Copy)]
struct View {
    width: u32,
    height: u32,
    scale: f32,
}

struct Inner {
    link: Option<Arc<HostLink>>,
    state: BrowserState,
    view: Option<View>,
    visible: bool,
    /// Counts the page loads that began, so a caller can tell its navigation from an earlier one.
    loads_started: u32,
}

/// What the page answered to a DevTools call: its result, or the message of its error.
type CdpAnswer = Result<Value, String>;

pub struct BrowserService {
    lynx_home: PathBuf,
    events: Arc<dyn BrowserEvents>,
    frames: Arc<dyn FrameSink>,
    inner: Mutex<Inner>,
    /// Starting the engine takes a while and the lock is released meanwhile; this keeps two
    /// callers from starting two engines.
    starting: Mutex<()>,
    state_changed: Condvar,
    /// Set while the engine is being stopped on purpose, so its closing is not reported as a crash.
    stopping: AtomicBool,
    /// DevTools calls waiting for their answer, by the id they went out with.
    cdp_waiting: Mutex<HashMap<u32, Sender<CdpAnswer>>>,
    next_cdp_id: AtomicU32,
    /// The pointer and the mark of an agent in control, drawn over the page by the view.
    pointer: Arc<AgentPointer>,
    /// A thread is waiting for the agent's control to end.
    watching: Arc<AtomicBool>,
    /// The page's console and requests, and what the engine answered by itself.
    log: Arc<PageLog>,
    /// One engine install at a time.
    installing: EngineInstallLock,
}

impl BrowserService {
    pub fn new(
        lynx_home: PathBuf,
        events: Arc<dyn BrowserEvents>,
        frames: Arc<dyn FrameSink>,
    ) -> Self {
        Self {
            lynx_home,
            events,
            frames,
            inner: Mutex::new(Inner {
                link: None,
                state: BrowserState::default(),
                view: None,
                visible: true,
                loads_started: 0,
            }),
            starting: Mutex::new(()),
            state_changed: Condvar::new(),
            stopping: AtomicBool::new(false),
            cdp_waiting: Mutex::new(HashMap::new()),
            next_cdp_id: AtomicU32::new(1),
            pointer: Arc::new(AgentPointer::default()),
            watching: Arc::new(AtomicBool::new(false)),
            log: Arc::new(PageLog::default()),
            installing: EngineInstallLock::default(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// What engine is installed, and whether this app can run it.
    pub fn engine_status(&self) -> EngineStatus {
        engine_files::read_status(&self.lynx_home)
    }

    /// Downloads the engine and puts it in place. Blocks until it is done, so call it off the
    /// async runtime. The running engine, if any, is stopped first: its files are about to change.
    pub fn install_engine(
        &self,
        force: bool,
        report: &mut dyn FnMut(EngineProgress),
    ) -> Result<EngineStatus, BrowserError> {
        let _only_one = self.installing.acquire().ok_or(EngineInstallError::Busy)?;
        install(&self.lynx_home, &manifest_url(), force, report)?;
        // An engine that failed to start, or was missing, is not a failure any more.
        let mut inner = self.lock();
        if inner.link.is_none() {
            self.set_state(&mut inner, BrowserState::default());
        }
        drop(inner);
        Ok(self.engine_status())
    }

    /// Stops the engine and deletes what the installer put on disk.
    pub fn remove_engine(&self) -> Result<EngineStatus, BrowserError> {
        let _only_one = self.installing.acquire().ok_or(EngineInstallError::Busy)?;
        self.shutdown();
        engine_files::remove(&self.lynx_home)
            .map_err(|error| EngineInstallError::io(&self.lynx_home.join("engine"), error))?;
        Ok(self.engine_status())
    }

    pub fn log(&self) -> Arc<PageLog> {
        Arc::clone(&self.log)
    }

    /// Turns on the DevTools domains the page log reads. Calls that need no answer, so it is safe
    /// to make from the link's own thread.
    fn enable_diagnostics(&self) {
        for method in [
            "Page.enable",
            "Runtime.enable",
            "Log.enable",
            "Network.enable",
        ] {
            let id = self.next_cdp_id.fetch_add(1, Ordering::Relaxed);
            let _ = self.send(ToHost::Cdp {
                id,
                method: method.to_owned(),
                params: Value::Null,
            });
        }
    }

    pub fn pointer(&self) -> Arc<AgentPointer> {
        Arc::clone(&self.pointer)
    }

    /// An agent starts a tool: its mark appears or stays, saying what it is about to do.
    pub fn agent_begins(&self, who: &Controller, label: &str) {
        let now = Instant::now();
        self.pointer.begin(who, label, now);
        // Whatever the agent does first, the person should see where it is: the pointer appears
        // in the middle of the page and moves from there.
        if !self.pointer.is_placed()
            && let Some(view) = self.lock().view
        {
            let middle = (f64::from(view.width) / 2.0, f64::from(view.height) / 2.0);
            self.pointer.move_to(middle, now);
        }
        self.frames.pointer_changed();
        self.watch_control();
    }

    /// The tool is done: the label stays as it is, and the dots stop.
    pub fn agent_ends(&self) {
        self.pointer.end(Instant::now());
        self.frames.pointer_changed();
    }

    /// Moves the agent's pointer to a point of the page, and waits until it gets there.
    pub fn agent_moves_to(&self, to: (f64, f64)) {
        let travel = self.pointer.move_to(to, Instant::now());
        self.frames.pointer_changed();
        std::thread::sleep(travel);
    }

    /// The pointer drifts while the agent reads or scrolls; the tool does not wait for it.
    pub fn agent_sweeps(&self, sweep: Sweep) {
        self.pointer.sweep(sweep, Instant::now());
        self.frames.pointer_changed();
    }

    /// The pointer becomes the arrow, or the text cursor over a field.
    pub fn agent_cursor(&self, cursor: CursorShape) {
        self.pointer.set_cursor(cursor);
        self.frames.pointer_changed();
    }

    /// The agent's pointer presses.
    pub fn agent_clicks(&self) {
        self.pointer.click(Instant::now());
        self.frames.pointer_changed();
    }

    /// Tells the interface once control is over. One watcher runs at a time, for as long as an
    /// agent is in control.
    fn watch_control(&self) {
        if self.watching.swap(true, Ordering::AcqRel) {
            return;
        }
        let pointer = Arc::clone(&self.pointer);
        let events = Arc::clone(&self.events);
        let watching = Arc::clone(&self.watching);
        let spawned = std::thread::Builder::new()
            .name("agent-control-watch".to_owned())
            .spawn(move || {
                while pointer.view(Instant::now()).is_some() {
                    std::thread::sleep(Duration::from_millis(200));
                }
                watching.store(false, Ordering::Release);
                events.agent_released();
            });
        if spawned.is_err() {
            self.watching.store(false, Ordering::Release);
        }
    }

    pub fn state(&self) -> BrowserState {
        self.lock().state.clone()
    }

    fn set_state(&self, inner: &mut Inner, mut state: BrowserState) {
        // Where the page is shown has nothing to do with what the engine reports.
        state.detached = inner.state.detached;
        if inner.state != state {
            inner.state = state;
            self.events.state_changed(&inner.state);
        }
        self.state_changed.notify_all();
    }

    /// Starts the engine if it is not running, waits until it is ready, and opens `url` when
    /// given. Blocks for the engine's start-up, so call it off the async runtime.
    pub fn open(self: &Arc<Self>, url: Option<&str>) -> Result<BrowserState, BrowserError> {
        let address = url.map(normalize).transpose()?;
        self.ensure_running()?;
        if let Some(address) = address {
            self.send(ToHost::Navigate { url: address })?;
        }
        Ok(self.state())
    }

    pub fn ensure_running(self: &Arc<Self>) -> Result<(), BrowserError> {
        let _one_at_a_time = self
            .starting
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut inner = self.lock();
        if inner.link.as_ref().is_some_and(|link| link.is_alive()) {
            return Ok(());
        }
        let host = locate(&self.lynx_home).inspect_err(|error| {
            let failed = BrowserState::failed(error.to_string());
            self.set_state(&mut inner, failed);
        })?;

        self.stopping.store(false, Ordering::Release);
        self.set_state(&mut inner, BrowserState::starting());

        // The callback runs before `spawn` has returned the link, so it reaches it through a cell
        // filled in right after. Frames only start once this function sends a command.
        let cell: Arc<OnceLock<Weak<HostLink>>> = Arc::new(OnceLock::new());
        let service = Arc::downgrade(self);
        let callback_cell = Arc::clone(&cell);
        // Starting the host takes a moment; do not hold the lock while it connects.
        drop(inner);
        let link = HostLink::spawn(&host, move |event| {
            if let Some(service) = service.upgrade() {
                service.on_link_event(event, callback_cell.get());
            }
        });
        let mut inner = self.lock();
        let link = match link {
            Ok(link) => Arc::new(link),
            Err(error) => {
                let error = BrowserError::from(error);
                self.set_state(&mut inner, BrowserState::failed(error.to_string()));
                return Err(error);
            }
        };
        let _ = cell.set(Arc::downgrade(&link));
        inner.link = Some(Arc::clone(&link));

        // The engine takes commands once it says it is ready.
        let (mut inner, timeout) = self
            .state_changed
            .wait_timeout_while(inner, READY_TIMEOUT, |inner| {
                inner.state.status == BrowserStatus::Starting
            })
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if inner.state.status != BrowserStatus::Running {
            let message = if timeout.timed_out() {
                "the browser engine did not become ready".to_owned()
            } else {
                inner
                    .state
                    .error
                    .clone()
                    .unwrap_or_else(|| "the browser engine stopped".to_owned())
            };
            inner.link = None;
            self.set_state(&mut inner, BrowserState::failed(message.clone()));
            return Err(BrowserError::EngineFailed(message));
        }
        // The domains the page log reads: on now, and again whenever the agent re-attaches.
        drop(inner);
        self.enable_diagnostics();
        let inner = self.lock();
        // What the interface already asked for before the engine was there.
        if let Some(view) = inner.view {
            let _ = link.send(&ToHost::Resize {
                width: view.width,
                height: view.height,
                scale: view.scale,
            });
        }
        let _ = link.send(&ToHost::Visible {
            visible: inner.visible,
        });
        Ok(())
    }

    /// Runs on the link's reader thread for everything the engine says.
    fn on_link_event(&self, event: LinkEvent, link: Option<&Weak<HostLink>>) {
        match event {
            LinkEvent::Message(FromHost::Frame {
                slot,
                width,
                height,
            }) => {
                let Some(link) = link.and_then(Weak::upgrade) else {
                    return;
                };
                let mut pixels = Vec::new();
                if link.copy_frame(slot, width, height, &mut pixels).is_ok() {
                    self.frames.frame(Frame {
                        width,
                        height,
                        pixels,
                    });
                }
                // Copied, so the engine may paint into the slot again.
                let _ = link.send(&ToHost::FrameDone { slot });
            }
            LinkEvent::Message(FromHost::CdpResult { id, ok, result }) => {
                let waiting = self.cdp_waiting().remove(&id);
                if let Some(waiting) = waiting {
                    let answer = if ok {
                        Ok(result)
                    } else {
                        Err(error_text(&result))
                    };
                    let _ = waiting.send(answer);
                }
            }
            LinkEvent::Message(FromHost::CdpEvent { method, params }) => {
                self.log.apply_event(&method, &params);
            }
            LinkEvent::Message(FromHost::Notice { text, .. }) => self.log.add_notice(&text),
            LinkEvent::Message(FromHost::DevToolsAttached) => self.enable_diagnostics(),
            LinkEvent::Message(message) => {
                let mut inner = self.lock();
                if matches!(message, FromHost::Loading { loading: true, .. }) {
                    inner.loads_started = inner.loads_started.wrapping_add(1);
                }
                let mut state = inner.state.clone();
                if state.apply(&message) {
                    self.set_state(&mut inner, state);
                } else {
                    self.state_changed.notify_all();
                }
            }
            LinkEvent::Closed => {
                self.pointer.release();
                // Dropping the senders wakes every caller still waiting for the page.
                self.cdp_waiting().clear();
                let mut inner = self.lock();
                inner.link = None;
                let state = if self.stopping.load(Ordering::Acquire) {
                    BrowserState::default()
                } else {
                    BrowserState::failed("the browser engine stopped")
                };
                self.set_state(&mut inner, state);
            }
        }
    }

    fn cdp_waiting(&self) -> MutexGuard<'_, HashMap<u32, Sender<CdpAnswer>>> {
        self.cdp_waiting
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// One DevTools call on the page, waiting up to `timeout` for the answer. Blocks, so call it
    /// off the async runtime.
    pub fn cdp(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, BrowserError> {
        let id = self.next_cdp_id.fetch_add(1, Ordering::Relaxed);
        let (answer, waiting) = mpsc::channel();
        self.cdp_waiting().insert(id, answer);
        let sent = self.send(ToHost::Cdp {
            id,
            method: method.to_owned(),
            params,
        });
        if let Err(error) = sent {
            self.cdp_waiting().remove(&id);
            return Err(error);
        }
        match waiting.recv_timeout(timeout) {
            Ok(answer) => answer.map_err(BrowserError::Page),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                self.cdp_waiting().remove(&id);
                Err(BrowserError::PageTimeout)
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(BrowserError::NotRunning),
        }
    }

    /// How many page loads have begun so far. Read it before starting a navigation, then hand it
    /// to [`BrowserService::wait_until_loaded`].
    pub fn loads_started(&self) -> u32 {
        self.lock().loads_started
    }

    /// Waits for the load that began after `loads_before` to finish. A navigation that never
    /// loads anything (a link to the same page, say) is given a moment to start and then ignored.
    pub fn wait_until_loaded(&self, loads_before: u32, timeout: Duration) {
        const GRACE: Duration = Duration::from_millis(600);
        let started = Instant::now();
        let mut inner = self.lock();
        loop {
            let begun = inner.loads_started != loads_before;
            let elapsed = started.elapsed();
            let settled = if begun {
                !inner.state.loading
            } else {
                elapsed >= GRACE
            };
            // While the page loads because of the agent, its label gives way to a ring.
            self.pointer.set_loading(begun && inner.state.loading);
            if settled || elapsed >= timeout || inner.link.is_none() {
                self.pointer.set_loading(false);
                return;
            }
            let remaining = timeout - elapsed;
            inner = self
                .state_changed
                .wait_timeout(inner, remaining.min(Duration::from_millis(100)))
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .0;
        }
    }

    /// Waits for a load that is under way to finish. Returns at once when nothing is loading.
    pub fn wait_until_idle(&self, timeout: Duration) {
        self.wait_until_loaded(self.loads_started().wrapping_sub(1), timeout);
    }

    /// Sends one command to the engine, if there is one.
    pub fn send(&self, command: ToHost) -> Result<(), BrowserError> {
        let link = self.lock().link.clone().ok_or(BrowserError::NotRunning)?;
        link.send(&command).map_err(BrowserError::from)
    }

    pub fn navigate(&self, input: &str) -> Result<(), BrowserError> {
        self.send(ToHost::Navigate {
            url: normalize(input)?,
        })
    }

    /// The size of the area the page is shown in, in logical pixels, and its screen's scale.
    pub fn resize(&self, width: u32, height: u32, scale: f32) {
        let view = View {
            width,
            height,
            scale,
        };
        let link = {
            let mut inner = self.lock();
            inner.view = Some(view);
            inner.link.clone()
        };
        if let Some(link) = link {
            let _ = link.send(&ToHost::Resize {
                width,
                height,
                scale,
            });
        }
    }

    /// A page nobody sees need not be painted.
    pub fn set_visible(&self, visible: bool) {
        let link = {
            let mut inner = self.lock();
            inner.visible = visible;
            inner.link.clone()
        };
        if let Some(link) = link {
            let _ = link.send(&ToHost::Visible { visible });
        }
    }

    /// Records that the page is shown in its own window (or back in the main one).
    pub fn set_detached(&self, detached: bool) {
        let mut inner = self.lock();
        if inner.state.detached != detached {
            inner.state.detached = detached;
            self.events.state_changed(&inner.state);
        }
    }

    /// Stops the engine. Safe to call when it is not running.
    pub fn shutdown(&self) {
        self.stopping.store(true, Ordering::Release);
        let link = self.lock().link.take();
        // Dropping the last handle stops the process, which can take a moment.
        drop(link);
        let mut inner = self.lock();
        self.set_state(&mut inner, BrowserState::default());
    }
}

/// The message in a DevTools error object, which looks like `{"code": -32000, "message": "..."}`.
fn error_text(error: &Value) -> String {
    error
        .get("message")
        .and_then(Value::as_str)
        .map_or_else(|| error.to_string(), str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Recorder(Mutex<Vec<BrowserState>>);

    impl BrowserEvents for Recorder {
        fn state_changed(&self, state: &BrowserState) {
            self.0.lock().unwrap().push(state.clone());
        }
    }

    struct Nowhere;

    impl FrameSink for Nowhere {
        fn frame(&self, _frame: Frame) {}
    }

    fn service(home: &std::path::Path) -> (Arc<BrowserService>, Arc<Recorder>) {
        let recorder = Arc::new(Recorder(Mutex::new(Vec::new())));
        let service = BrowserService::new(
            home.to_path_buf(),
            Arc::clone(&recorder) as Arc<dyn BrowserEvents>,
            Arc::new(Nowhere),
        );
        (Arc::new(service), recorder)
    }

    #[test]
    fn opening_without_an_engine_fails_and_says_so() {
        let home = tempfile::tempdir().unwrap();
        let (service, recorder) = service(home.path());
        let error = service.open(Some("example.com")).unwrap_err();
        assert!(matches!(error, BrowserError::EngineMissing(_)), "{error}");
        let state = service.state();
        assert_eq!(state.status, BrowserStatus::Failed);
        assert!(state.error.unwrap().contains("not installed"));
        assert_eq!(
            recorder.0.lock().unwrap().len(),
            1,
            "the interface was told once"
        );
    }

    #[test]
    fn a_bad_address_is_refused_before_anything_starts() {
        let home = tempfile::tempdir().unwrap();
        let (service, recorder) = service(home.path());
        let error = service.open(Some("file:///etc/passwd")).unwrap_err();
        assert!(matches!(error, BrowserError::InvalidAddress));
        assert!(
            recorder.0.lock().unwrap().is_empty(),
            "no engine was looked for"
        );
    }

    #[test]
    fn commands_need_a_running_engine() {
        let home = tempfile::tempdir().unwrap();
        let (service, _) = service(home.path());
        assert!(matches!(
            service.send(ToHost::Back),
            Err(BrowserError::NotRunning)
        ));
        assert!(matches!(
            service.navigate("example.com"),
            Err(BrowserError::NotRunning)
        ));
    }

    #[test]
    fn a_view_size_given_early_is_remembered() {
        let home = tempfile::tempdir().unwrap();
        let (service, _) = service(home.path());
        service.resize(480, 320, 2.0);
        let view = service.lock().view.unwrap();
        assert_eq!((view.width, view.height, view.scale), (480, 320, 2.0));
    }

    // Needs the engine installed under the real Lynx home (`bun run browser:engine`). Run with
    // `cargo test -p lynx-ide the_service_drives_a_real_engine -- --ignored --nocapture`.
    #[test]
    #[ignore = "needs the installed browser engine"]
    fn the_service_drives_a_real_engine() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::atomic::AtomicUsize;
        use std::time::Instant;

        struct Counting(Arc<AtomicUsize>);
        impl FrameSink for Counting {
            fn frame(&self, frame: Frame) {
                assert_eq!(
                    frame.pixels.len(),
                    frame.width as usize * frame.height as usize * 4
                );
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }

        // A page server of our own: nothing here depends on the internet.
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = server.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for stream in server.incoming().flatten() {
                std::thread::spawn(move || {
                    let mut stream = stream;
                    let mut request = [0u8; 2048];
                    let read = stream.read(&mut request).unwrap_or(0);
                    let request = String::from_utf8_lossy(&request[..read]);
                    let title = if request.starts_with("GET /second") {
                        "Second"
                    } else {
                        "Hello from Lynx"
                    };
                    let body = format!(
                        "<!doctype html><title>{title}</title><body style='background:#fff'><h1>{title}</h1>"
                    );
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                });
            }
        });

        let home = std::env::var_os("LYNX_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap()).join(".lynx"));
        let frames = Arc::new(AtomicUsize::new(0));
        let recorder = Arc::new(Recorder(Mutex::new(Vec::new())));
        let service = Arc::new(BrowserService::new(
            home,
            Arc::clone(&recorder) as Arc<dyn BrowserEvents>,
            Arc::new(Counting(Arc::clone(&frames))),
        ));

        let wait_until = |what: &str, condition: &dyn Fn() -> bool| {
            let started = Instant::now();
            while !condition() {
                assert!(
                    started.elapsed() < Duration::from_secs(15),
                    "timed out waiting for {what}"
                );
                std::thread::sleep(Duration::from_millis(25));
            }
        };

        // The size is given before the engine exists, and must reach it once it does.
        service.resize(400, 300, 1.0);
        let state = service
            .open(Some(&format!("127.0.0.1:{port}")))
            .expect("the engine starts");
        assert_eq!(state.status, BrowserStatus::Running);

        wait_until("the first page", &|| {
            service.state().title == "Hello from Lynx"
        });
        wait_until("frames", &|| frames.load(Ordering::Relaxed) > 0);
        let state = service.state();
        assert!(
            state.url.starts_with(&format!("http://127.0.0.1:{port}")),
            "{}",
            state.url
        );

        service
            .navigate(&format!("127.0.0.1:{port}/second"))
            .unwrap();
        wait_until("the second page", &|| service.state().title == "Second");
        wait_until("history", &|| service.state().can_go_back);

        service.send(ToHost::Back).unwrap();
        wait_until("going back", &|| service.state().title == "Hello from Lynx");

        assert!(
            recorder
                .0
                .lock()
                .unwrap()
                .iter()
                .any(|state| state.status == BrowserStatus::Starting),
            "the interface saw the engine start"
        );

        service.shutdown();
        assert_eq!(service.state().status, BrowserStatus::Stopped);
        assert!(matches!(
            service.send(ToHost::Back),
            Err(BrowserError::NotRunning)
        ));
    }

    #[test]
    fn where_the_page_is_shown_is_told_once_and_survives_other_changes() {
        let home = tempfile::tempdir().unwrap();
        let (service, recorder) = service(home.path());

        service.set_detached(true);
        service.set_detached(true);
        assert_eq!(
            recorder.0.lock().unwrap().len(),
            1,
            "an unchanged value is not news"
        );
        assert!(service.state().detached);

        // A failed start replaces the whole state; the window it is shown in must not be lost.
        let _ = service.open(Some("example.com"));
        let state = service.state();
        assert_eq!(state.status, BrowserStatus::Failed);
        assert!(state.detached);

        service.set_detached(false);
        assert!(!service.state().detached);
    }

    #[test]
    fn shutting_down_when_nothing_runs_is_harmless() {
        let home = tempfile::tempdir().unwrap();
        let (service, _) = service(home.path());
        service.shutdown();
        service.shutdown();
        assert_eq!(service.state().status, BrowserStatus::Stopped);
    }
}
