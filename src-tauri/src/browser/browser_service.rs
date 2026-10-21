//! Owns the browser engine process and what Lynx knows about its pages. The engine starts when a
//! page is first needed and stops with the app; if it goes away on its own, the state says so and
//! the next use starts a new one. Every page has an owner (see `page_owner.rs`). The person sees
//! one page at a time, the shown page; an agent works on the page its owner uses.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, Weak};
use std::time::{Duration, Instant};

use serde_json::Value;

use lynx_browser_protocol::link::{HostLink, LinkEvent};
use lynx_browser_protocol::message::{FromHost, PageCommand, PageEvent, PageId, ToHost};

use super::agent_pointer::{AgentPointer, Controller, CursorShape, Sweep};
use super::browser_error::BrowserError;
use super::browser_state::{BrowserPageInfo, BrowserState, BrowserStatus, BrowserThumbnail};
use super::engine_files::{self, EngineStatus};
use super::engine_install_error::EngineInstallError;
use super::engine_installer::{EngineInstallLock, EngineProgress, install};
use super::engine_location::locate;
use super::engine_manifest::manifest_url;
use super::page_log::PageLog;
use super::page_owner::PageOwner;
use super::page_thumbnail;
use super::web_address::normalize;

/// How long a freshly started engine has to say it is ready.
const READY_TIMEOUT: Duration = Duration::from_secs(20);
/// How long a new page has to exist.
const PAGE_TIMEOUT: Duration = Duration::from_secs(10);
/// How long a page has to answer the domains the page log reads, when it is first made.
const DIAGNOSTICS_TIMEOUT: Duration = Duration::from_secs(2);
/// The shortest time between two pictures of one page in the list.
const THUMBNAIL_INTERVAL: Duration = Duration::from_secs(1);

/// The DevTools domains the page log reads, turned on for every page.
const DIAGNOSTIC_DOMAINS: [&str; 4] = [
    "Page.enable",
    "Runtime.enable",
    "Log.enable",
    "Network.enable",
];

/// One frame of a page, BGRA, in device pixels.
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// Where Lynx tells the interface about changes.
pub trait BrowserEvents: Send + Sync + 'static {
    /// The state of the page the person sees.
    fn state_changed(&self, state: &BrowserState);

    /// The list of pages, for every owner, changed.
    fn pages_changed(&self, _pages: &[BrowserPageInfo]) {}

    /// A page has a new picture for the list.
    fn thumbnail_changed(&self, _page: PageId, _png: &str) {}

    /// No agent is in control of a page any more.
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

/// One page of the engine, as Lynx knows it.
struct Page {
    owner: PageOwner,
    /// The engine has said the page exists; commands sent before that would be lost.
    created: bool,
    /// What the page shows: its address, title, loading and history.
    content: BrowserState,
    /// Counts the page loads that began, so a caller can tell its navigation from an earlier one.
    loads_started: u32,
    log: Arc<PageLog>,
    /// The pointer and the mark of an agent in control, drawn over the page by the view.
    pointer: Arc<AgentPointer>,
    /// One agent tool call at a time on this page: two agents must not interleave their clicks.
    turn: Arc<Mutex<()>>,
    /// A thread is waiting for the agent's control of this page to end.
    watching: Arc<AtomicBool>,
    /// The harness of the agent in control of the page, while it is.
    agent: Option<String>,
    /// The last picture of the page, as base64 PNG, and when it was made.
    thumbnail: Option<String>,
    thumbnail_at: Option<Instant>,
}

impl Page {
    fn new(owner: PageOwner) -> Self {
        Self {
            owner,
            created: false,
            content: BrowserState::default(),
            loads_started: 0,
            log: Arc::new(PageLog::default()),
            pointer: Arc::new(AgentPointer::default()),
            turn: Arc::new(Mutex::new(())),
            watching: Arc::new(AtomicBool::new(false)),
            agent: None,
            thumbnail: None,
            thumbnail_at: None,
        }
    }

    fn info(&self, id: PageId) -> BrowserPageInfo {
        BrowserPageInfo {
            id,
            owner: self.owner.clone(),
            url: self.content.url.clone(),
            title: self.content.title.clone(),
            loading: self.content.loading,
            agent: self.agent.clone(),
        }
    }
}

struct Inner {
    link: Option<Arc<HostLink>>,
    /// The engine's own status and the error it failed with, if any.
    engine: BrowserStatus,
    engine_error: Option<String>,
    pages: HashMap<PageId, Page>,
    /// The page each owner's agents use.
    agent_pages: HashMap<PageOwner, PageId>,
    /// The page the person sees.
    shown: Option<PageId>,
    next_page: PageId,
    /// The size of the area the person sees; every page is given it.
    view: Option<View>,
    /// Whether the person sees the browser now; only the shown page is ever drawn.
    visible: bool,
    detached: bool,
    /// The state the interface was last told about, so an unchanged one is not sent again.
    published: BrowserState,
    /// The page list the interface was last told about, for the same reason.
    published_pages: Vec<BrowserPageInfo>,
}

impl Inner {
    /// The state of the engine, and of `page`'s content when there is one.
    fn describe(&self, page: Option<PageId>) -> BrowserState {
        let content = page
            .and_then(|page| self.pages.get(&page))
            .map(|page| page.content.clone())
            .unwrap_or_default();
        BrowserState {
            status: self.engine,
            url: content.url,
            title: content.title,
            loading: content.loading,
            can_go_back: content.can_go_back,
            can_go_forward: content.can_go_forward,
            error: self.engine_error.clone().or(content.error),
            detached: self.detached,
            page: self.shown,
        }
    }

    fn page_list(&self) -> Vec<BrowserPageInfo> {
        let mut ids: Vec<PageId> = self.pages.keys().copied().collect();
        ids.sort_unstable();
        ids.iter()
            .filter_map(|id| self.pages.get(id).map(|page| page.info(*id)))
            .collect()
    }
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
                engine: BrowserStatus::Stopped,
                engine_error: None,
                pages: HashMap::new(),
                agent_pages: HashMap::new(),
                shown: None,
                next_page: 1,
                view: None,
                visible: true,
                detached: false,
                published: BrowserState::default(),
                published_pages: Vec::new(),
            }),
            starting: Mutex::new(()),
            state_changed: Condvar::new(),
            stopping: AtomicBool::new(false),
            cdp_waiting: Mutex::new(HashMap::new()),
            next_cdp_id: AtomicU32::new(1),
            installing: EngineInstallLock::default(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn cdp_waiting(&self) -> MutexGuard<'_, HashMap<u32, Sender<CdpAnswer>>> {
        self.cdp_waiting
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
        if cfg!(windows) {
            // Windows will not replace a folder with a running program in it.
            self.shutdown();
        }
        install(&self.lynx_home, &manifest_url(), force, report)?;
        // An engine that failed to start, or was missing, is not a failure any more.
        let mut inner = self.lock();
        if inner.link.is_none() {
            inner.engine = BrowserStatus::Stopped;
            inner.engine_error = None;
            self.publish(&mut inner);
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

    /// The state the interface shows: the shown page on top of the engine's status.
    pub fn state(&self) -> BrowserState {
        let inner = self.lock();
        inner.describe(inner.shown)
    }

    /// The state of one page, for the agent working on it.
    pub fn state_of(&self, page: PageId) -> BrowserState {
        self.lock().describe(Some(page))
    }

    /// Every page, for every owner, in the order they were made.
    pub fn pages(&self) -> Vec<BrowserPageInfo> {
        self.lock().page_list()
    }

    /// The last picture of every page that has one.
    pub fn thumbnails(&self) -> Vec<BrowserThumbnail> {
        let inner = self.lock();
        let mut thumbnails: Vec<BrowserThumbnail> = inner
            .pages
            .iter()
            .filter_map(|(id, page)| {
                page.thumbnail.as_ref().map(|png| BrowserThumbnail {
                    page: *id,
                    png: png.clone(),
                })
            })
            .collect();
        thumbnails.sort_by_key(|thumbnail| thumbnail.page);
        thumbnails
    }

    /// Whether `page` is the one the person sees.
    pub fn is_shown(&self, page: PageId) -> bool {
        self.lock().shown == Some(page)
    }

    /// Tells the interface when what it shows has changed, and only then.
    fn publish(&self, inner: &mut Inner) {
        let state = inner.describe(inner.shown);
        if state != inner.published {
            inner.published = state.clone();
            self.events.state_changed(&state);
        }
        let pages = inner.page_list();
        if pages != inner.published_pages {
            inner.published_pages = pages.clone();
            self.events.pages_changed(&pages);
        }
        self.state_changed.notify_all();
    }

    fn fail_engine(&self, inner: &mut Inner, message: String) {
        inner.engine = BrowserStatus::Failed;
        inner.engine_error = Some(message);
        self.publish(inner);
    }

    /// Starts the engine if it is not running, and waits until it is ready. Blocks for the
    /// engine's start-up, so call it off the async runtime.
    pub fn ensure_running(self: &Arc<Self>) -> Result<(), BrowserError> {
        let _one_at_a_time = self
            .starting
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut inner = self.lock();
        if inner.link.as_ref().is_some_and(|link| link.is_alive()) {
            return Ok(());
        }
        let host = locate(&self.lynx_home)
            .inspect_err(|error| self.fail_engine(&mut inner, error.to_string()))?;

        self.stopping.store(false, Ordering::Release);
        inner.engine = BrowserStatus::Starting;
        inner.engine_error = None;
        self.publish(&mut inner);

        // The callback runs before `spawn` has returned the link, so it reaches it through a cell
        // filled in right after. Frames only start once a page exists.
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
                self.fail_engine(&mut inner, error.to_string());
                return Err(error);
            }
        };
        let _ = cell.set(Arc::downgrade(&link));
        inner.link = Some(Arc::clone(&link));

        // The engine takes commands once it says it is ready.
        let (mut inner, timeout) = self
            .state_changed
            .wait_timeout_while(inner, READY_TIMEOUT, |inner| {
                inner.engine == BrowserStatus::Starting
            })
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if inner.engine != BrowserStatus::Running {
            let message = if timeout.timed_out() {
                "the browser engine did not become ready".to_owned()
            } else {
                inner
                    .engine_error
                    .clone()
                    .unwrap_or_else(|| "the browser engine stopped".to_owned())
            };
            inner.link = None;
            self.fail_engine(&mut inner, message.clone());
            return Err(BrowserError::EngineFailed(message));
        }
        Ok(())
    }

    /// The page an owner's agents use, made the first time an agent needs one. Blocks until the
    /// engine has made it, so call it off the async runtime.
    pub fn page_for(self: &Arc<Self>, owner: &PageOwner) -> Result<PageId, BrowserError> {
        self.ensure_running()?;
        self.make_page(owner.clone(), true)
    }

    /// A new page for an owner, shown to the person and navigated to `url` when given. Blocks
    /// until the engine has made it, so call it off the async runtime.
    pub fn new_page(
        self: &Arc<Self>,
        owner: PageOwner,
        url: Option<&str>,
    ) -> Result<BrowserPageInfo, BrowserError> {
        let address = url.map(normalize).transpose()?;
        self.ensure_running()?;
        let page = self.make_page(owner, false)?;
        self.show_page(page)?;
        if let Some(address) = address {
            self.send(page, PageCommand::Navigate { url: address })?;
        }
        self.page_info(page).ok_or(BrowserError::NotRunning)
    }

    /// The registry's view of one page.
    pub fn page_info(&self, page: PageId) -> Option<BrowserPageInfo> {
        self.lock().pages.get(&page).map(|entry| entry.info(page))
    }

    /// Makes a page for `owner`, or returns the owner's agent page when `agent` is set and it
    /// exists. The engine must be running.
    fn make_page(self: &Arc<Self>, owner: PageOwner, agent: bool) -> Result<PageId, BrowserError> {
        let page = {
            let mut inner = self.lock();
            if agent && let Some(existing) = inner.agent_pages.get(&owner).copied() {
                drop(inner);
                self.wait_created(existing)?;
                return Ok(existing);
            }
            let page = inner.next_page;
            inner.next_page += 1;
            inner.pages.insert(page, Page::new(owner.clone()));
            if agent {
                inner.agent_pages.insert(owner, page);
            }
            page
        };
        let link = self.lock().link.clone().ok_or(BrowserError::NotRunning)?;
        if let Err(error) = link.send(&ToHost::CreatePage { page }) {
            self.forget_page(page);
            return Err(error.into());
        }
        if let Err(error) = self.wait_created(page) {
            self.forget_page(page);
            return Err(error);
        }
        // A request sent before the page log's domains are on is never seen, so they are awaited
        // here. A refused or unanswered call is not an error: a later attach turns them on again.
        for method in DIAGNOSTIC_DOMAINS {
            let _ = self.cdp(page, method, Value::Null, DIAGNOSTICS_TIMEOUT);
        }
        Ok(page)
    }

    /// Waits until the engine has made `page`.
    fn wait_created(&self, page: PageId) -> Result<(), BrowserError> {
        let inner = self.lock();
        let (inner, timeout) = self
            .state_changed
            .wait_timeout_while(inner, PAGE_TIMEOUT, |inner| {
                inner.pages.get(&page).is_some_and(|entry| !entry.created)
            })
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if inner.pages.get(&page).is_some_and(|entry| entry.created) {
            return Ok(());
        }
        let message = if timeout.timed_out() {
            "the browser engine did not make the page"
        } else {
            "the browser engine stopped"
        };
        Err(BrowserError::EngineFailed(message.to_owned()))
    }

    /// Drops a page Lynx has not got from the engine, so the owner can ask for a new one.
    fn forget_page(&self, page: PageId) {
        let mut inner = self.lock();
        inner.pages.remove(&page);
        inner.agent_pages.retain(|_, owned| *owned != page);
        if inner.shown == Some(page) {
            inner.shown = None;
        }
    }

    /// Shows `page` to the person, and hides the page that was shown before.
    pub fn show_page(&self, page: PageId) -> Result<BrowserState, BrowserError> {
        let (link, previous, visible) = {
            let mut inner = self.lock();
            if !inner.pages.get(&page).is_some_and(|entry| entry.created) {
                return Err(BrowserError::NotRunning);
            }
            let previous = inner.shown.replace(page);
            (inner.link.clone(), previous, inner.visible)
        };
        if let Some(link) = link {
            if let Some(previous) = previous.filter(|previous| *previous != page) {
                let _ = link.send(&ToHost::Page {
                    page: previous,
                    command: PageCommand::Visible { visible: false },
                });
            }
            let _ = link.send(&ToHost::Page {
                page,
                command: PageCommand::Visible { visible },
            });
        }
        {
            let mut inner = self.lock();
            self.publish(&mut inner);
        }
        Ok(self.state())
    }

    /// Closes a page. Nothing is shown afterwards if it was the shown one. A page that does not
    /// exist is already closed.
    pub fn close_page(&self, page: PageId) -> Result<(), BrowserError> {
        let link = {
            let mut inner = self.lock();
            if !inner.pages.contains_key(&page) {
                return Ok(());
            }
            let link = inner.link.clone().ok_or(BrowserError::NotRunning)?;
            if inner.shown == Some(page) {
                inner.shown = None;
            }
            inner.pages.remove(&page);
            inner.agent_pages.retain(|_, owned| *owned != page);
            self.publish(&mut inner);
            link
        };
        link.send(&ToHost::ClosePage { page })
            .map_err(BrowserError::from)
    }

    /// Starts the engine if needed. When an address is given and a page is shown, that page goes
    /// to it. Blocks for the engine's start-up, so call it off the async runtime.
    pub fn open(self: &Arc<Self>, url: Option<&str>) -> Result<BrowserState, BrowserError> {
        let address = url.map(normalize).transpose()?;
        self.ensure_running()?;
        let shown = self.lock().shown;
        if let (Some(address), Some(page)) = (address, shown) {
            self.send(page, PageCommand::Navigate { url: address })?;
        }
        Ok(self.state())
    }

    /// Goes to what was typed in the address bar of the shown page.
    pub fn navigate(&self, input: &str) -> Result<(), BrowserError> {
        let url = normalize(input)?;
        self.send_shown(PageCommand::Navigate { url })
    }

    /// Sends one command to the shown page.
    pub fn send_shown(&self, command: PageCommand) -> Result<(), BrowserError> {
        let page = self.lock().shown.ok_or(BrowserError::NotRunning)?;
        self.send(page, command)
    }

    /// Sends one command to a page, if the engine is running.
    pub fn send(&self, page: PageId, command: PageCommand) -> Result<(), BrowserError> {
        self.send_link(&ToHost::Page { page, command })
    }

    fn send_link(&self, message: &ToHost) -> Result<(), BrowserError> {
        let link = self.lock().link.clone().ok_or(BrowserError::NotRunning)?;
        link.send(message).map_err(BrowserError::from)
    }

    /// The DevTools domains the page log reads, on for one page.
    fn diagnostic_calls(&self, page: PageId) -> Vec<ToHost> {
        DIAGNOSTIC_DOMAINS
            .iter()
            .map(|method| ToHost::Page {
                page,
                command: PageCommand::Cdp {
                    id: self.next_cdp_id.fetch_add(1, Ordering::Relaxed),
                    method: (*method).to_owned(),
                    params: Value::Null,
                },
            })
            .collect()
    }

    /// Turns on the diagnostics of one page. Calls that need no answer, so it is safe to make
    /// from the link's own thread.
    fn enable_diagnostics(&self, page: PageId) {
        for call in self.diagnostic_calls(page) {
            let _ = self.send_link(&call);
        }
    }

    /// The page's log of console and requests.
    pub fn log_of(&self, page: PageId) -> Arc<PageLog> {
        self.lock().pages.get(&page).map_or_else(
            || Arc::new(PageLog::default()),
            |entry| Arc::clone(&entry.log),
        )
    }

    /// The gate that keeps one agent tool call at a time on a page.
    pub fn turn_of(&self, page: PageId) -> Option<Arc<Mutex<()>>> {
        self.lock()
            .pages
            .get(&page)
            .map(|entry| Arc::clone(&entry.turn))
    }

    /// The pointer drawn over the shown page, if there is one.
    pub fn active_pointer(&self) -> Option<Arc<AgentPointer>> {
        let inner = self.lock();
        inner
            .shown
            .and_then(|page| inner.pages.get(&page))
            .map(|entry| Arc::clone(&entry.pointer))
    }

    fn pointer_of(&self, page: PageId) -> Option<Arc<AgentPointer>> {
        self.lock()
            .pages
            .get(&page)
            .map(|entry| Arc::clone(&entry.pointer))
    }

    /// The pointer and the label of an agent that starts a tool on a page.
    pub fn agent_begins(self: &Arc<Self>, page: PageId, who: &Controller, label: &str) {
        let Some(pointer) = self.pointer_of(page) else {
            return;
        };
        let now = Instant::now();
        pointer.begin(who, label, now);
        // Whatever the agent does first, the person should see where it is: the pointer appears
        // in the middle of the page and moves from there.
        if !pointer.is_placed()
            && let Some(view) = self.lock().view
        {
            let middle = (f64::from(view.width) / 2.0, f64::from(view.height) / 2.0);
            pointer.move_to(middle, now);
        }
        {
            let mut inner = self.lock();
            if let Some(entry) = inner.pages.get_mut(&page) {
                entry.agent = Some(who.harness.clone());
            }
            self.publish(&mut inner);
        }
        self.repaint(page);
        self.watch_control(page);
    }

    /// The tool is done: the label stays as it is, and the dots stop.
    pub fn agent_ends(&self, page: PageId) {
        if let Some(pointer) = self.pointer_of(page) {
            pointer.end(Instant::now());
        }
        self.repaint(page);
    }

    /// Moves the agent's pointer to a point of the page, and waits until it gets there.
    pub fn agent_moves_to(&self, page: PageId, to: (f64, f64)) {
        let Some(pointer) = self.pointer_of(page) else {
            return;
        };
        let travel = pointer.move_to(to, Instant::now());
        self.repaint(page);
        std::thread::sleep(travel);
    }

    /// The pointer drifts while the agent reads or scrolls; the tool does not wait for it.
    pub fn agent_sweeps(&self, page: PageId, sweep: Sweep) {
        if let Some(pointer) = self.pointer_of(page) {
            pointer.sweep(sweep, Instant::now());
        }
        self.repaint(page);
    }

    /// The pointer becomes the arrow, or the text cursor over a field.
    pub fn agent_cursor(&self, page: PageId, cursor: CursorShape) {
        if let Some(pointer) = self.pointer_of(page) {
            pointer.set_cursor(cursor);
        }
        self.repaint(page);
    }

    /// The agent's pointer presses.
    pub fn agent_clicks(&self, page: PageId) {
        if let Some(pointer) = self.pointer_of(page) {
            pointer.click(Instant::now());
        }
        self.repaint(page);
    }

    /// The view repaints only for the page the person sees.
    fn repaint(&self, page: PageId) {
        if self.is_shown(page) {
            self.frames.pointer_changed();
        }
    }

    /// Forgets the agent in control of a page, once its control is over.
    fn clear_agent(&self, page: PageId) {
        let mut inner = self.lock();
        if let Some(entry) = inner.pages.get_mut(&page) {
            entry.agent = None;
        }
        self.publish(&mut inner);
    }

    /// Tells the interface once control is over. One watcher runs at a time per page, for as long
    /// as an agent is in control of it.
    fn watch_control(self: &Arc<Self>, page: PageId) {
        let Some((pointer, watching)) = self
            .lock()
            .pages
            .get(&page)
            .map(|entry| (Arc::clone(&entry.pointer), Arc::clone(&entry.watching)))
        else {
            return;
        };
        if watching.swap(true, Ordering::AcqRel) {
            return;
        }
        let events = Arc::clone(&self.events);
        let flag = Arc::clone(&watching);
        let service = Arc::downgrade(self);
        let spawned = std::thread::Builder::new()
            .name("agent-control-watch".to_owned())
            .spawn(move || {
                while pointer.view(Instant::now()).is_some() {
                    std::thread::sleep(Duration::from_millis(200));
                }
                flag.store(false, Ordering::Release);
                if let Some(service) = service.upgrade() {
                    service.clear_agent(page);
                }
                events.agent_released();
            });
        if spawned.is_err() {
            watching.store(false, Ordering::Release);
        }
    }

    /// Runs on the link's reader thread for everything the engine says.
    fn on_link_event(&self, event: LinkEvent, link: Option<&Weak<HostLink>>) {
        match event {
            LinkEvent::Message(FromHost::Ready) => {
                let mut inner = self.lock();
                inner.engine = BrowserStatus::Running;
                self.publish(&mut inner);
            }
            LinkEvent::Message(FromHost::PageCreated { page }) => self.on_page_created(page),
            LinkEvent::Message(FromHost::PageClosed { page }) => {
                let mut inner = self.lock();
                inner.pages.remove(&page);
                inner.agent_pages.retain(|_, owned| *owned != page);
                if inner.shown == Some(page) {
                    inner.shown = None;
                }
                self.publish(&mut inner);
            }
            LinkEvent::Message(FromHost::Page { page, event }) => {
                self.on_page_event(page, event, link);
            }
            LinkEvent::Closed => {
                let stopping = self.stopping.load(Ordering::Acquire);
                let mut inner = self.lock();
                for entry in inner.pages.values() {
                    entry.pointer.release();
                }
                inner.pages.clear();
                inner.agent_pages.clear();
                inner.shown = None;
                inner.link = None;
                inner.engine = if stopping {
                    BrowserStatus::Stopped
                } else {
                    BrowserStatus::Failed
                };
                inner.engine_error = (!stopping).then(|| "the browser engine stopped".to_owned());
                self.cdp_waiting().clear();
                self.publish(&mut inner);
            }
        }
    }

    /// The engine made a page. What was asked for before it existed is sent first, then callers
    /// waiting for it are woken.
    fn on_page_created(&self, page: PageId) {
        let mut inner = self.lock();
        let shown = inner.shown == Some(page) && inner.visible;
        let view = inner.view;
        if let Some(link) = inner.link.clone() {
            let _ = link.send(&ToHost::Page {
                page,
                command: PageCommand::Visible { visible: shown },
            });
            if let Some(view) = view {
                let _ = link.send(&ToHost::Page {
                    page,
                    command: PageCommand::Resize {
                        width: view.width,
                        height: view.height,
                        scale: view.scale,
                    },
                });
            }
        }
        if let Some(entry) = inner.pages.get_mut(&page) {
            entry.created = true;
        }
        self.state_changed.notify_all();
    }

    fn on_page_event(&self, page: PageId, event: PageEvent, link: Option<&Weak<HostLink>>) {
        match event {
            PageEvent::Frame {
                slot,
                width,
                height,
            } => {
                let Some(link) = link.and_then(Weak::upgrade) else {
                    return;
                };
                // A frame is copied only when the person sees it, or when the list needs a picture.
                let (shown, due) = {
                    let inner = self.lock();
                    (
                        inner.shown == Some(page),
                        inner.pages.get(&page).is_some_and(|entry| {
                            entry
                                .thumbnail_at
                                .is_none_or(|at| at.elapsed() >= THUMBNAIL_INTERVAL)
                        }),
                    )
                };
                if shown || due {
                    let mut pixels = Vec::new();
                    if link.copy_frame(slot, width, height, &mut pixels).is_ok() {
                        if due && let Some(png) = page_thumbnail::encode(width, height, &pixels) {
                            self.store_thumbnail(page, &png);
                        }
                        if shown {
                            self.frames.frame(Frame {
                                width,
                                height,
                                pixels,
                            });
                        }
                    }
                }
                // Copied, so the engine may paint into the slot again.
                let _ = link.send(&ToHost::FrameDone { slot });
            }
            PageEvent::CdpResult { id, ok, result } => {
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
            PageEvent::CdpEvent { method, params } => {
                let log = self.log_of(page);
                log.apply_event(&method, &params);
            }
            PageEvent::Notice { text, .. } => {
                let log = self.log_of(page);
                log.add_notice(&text);
            }
            PageEvent::DevToolsAttached => self.enable_diagnostics(page),
            other => {
                let mut inner = self.lock();
                if let Some(entry) = inner.pages.get_mut(&page) {
                    if matches!(other, PageEvent::Loading { loading: true, .. }) {
                        entry.loads_started = entry.loads_started.wrapping_add(1);
                    }
                    entry.content.apply(&other);
                }
                self.publish(&mut inner);
            }
        }
    }

    /// Keeps the picture of a page for the list, and tells the interface about it.
    fn store_thumbnail(&self, page: PageId, png: &str) {
        {
            let mut inner = self.lock();
            let Some(entry) = inner.pages.get_mut(&page) else {
                return;
            };
            entry.thumbnail = Some(png.to_owned());
            entry.thumbnail_at = Some(Instant::now());
        }
        self.events.thumbnail_changed(page, png);
    }

    /// How many page loads have begun so far. Read it before starting a navigation, then hand it
    /// to [`BrowserService::wait_until_loaded`].
    pub fn loads_started(&self, page: PageId) -> u32 {
        self.lock()
            .pages
            .get(&page)
            .map_or(0, |entry| entry.loads_started)
    }

    /// Waits for the load that began after `loads_before` to finish. A navigation that never
    /// loads anything (a link to the same page, say) is given a moment to start and then ignored.
    pub fn wait_until_loaded(&self, page: PageId, loads_before: u32, timeout: Duration) {
        const GRACE: Duration = Duration::from_millis(600);
        let started = Instant::now();
        let pointer = self.pointer_of(page);
        let mut inner = self.lock();
        while let Some(entry) = inner.pages.get(&page) {
            let begun = entry.loads_started != loads_before;
            let loading = entry.content.loading;
            let elapsed = started.elapsed();
            let settled = if begun { !loading } else { elapsed >= GRACE };
            // While the page loads because of the agent, its label gives way to a ring.
            if let Some(pointer) = &pointer {
                pointer.set_loading(begun && loading);
            }
            if settled || elapsed >= timeout || inner.link.is_none() {
                break;
            }
            let remaining = timeout - elapsed;
            inner = self
                .state_changed
                .wait_timeout(inner, remaining.min(Duration::from_millis(100)))
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .0;
        }
        if let Some(pointer) = &pointer {
            pointer.set_loading(false);
        }
    }

    /// Waits for a load that is under way to finish. Returns at once when nothing is loading.
    pub fn wait_until_idle(&self, page: PageId, timeout: Duration) {
        self.wait_until_loaded(page, self.loads_started(page).wrapping_sub(1), timeout);
    }

    /// One DevTools call on a page, waiting up to `timeout` for the answer. Blocks, so call it
    /// off the async runtime.
    pub fn cdp(
        &self,
        page: PageId,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, BrowserError> {
        let id = self.next_cdp_id.fetch_add(1, Ordering::Relaxed);
        let (answer, waiting) = mpsc::channel();
        self.cdp_waiting().insert(id, answer);
        let sent = self.send(
            page,
            PageCommand::Cdp {
                id,
                method: method.to_owned(),
                params,
            },
        );
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

    /// The size of the area the person sees, in logical pixels, and its screen's scale. Every page
    /// is given it.
    pub fn resize(&self, width: u32, height: u32, scale: f32) {
        let view = View {
            width,
            height,
            scale,
        };
        let (link, pages) = {
            let mut inner = self.lock();
            inner.view = Some(view);
            let pages: Vec<PageId> = inner
                .pages
                .iter()
                .filter(|(_, entry)| entry.created)
                .map(|(page, _)| *page)
                .collect();
            (inner.link.clone(), pages)
        };
        if let Some(link) = link {
            for page in pages {
                let _ = link.send(&ToHost::Page {
                    page,
                    command: PageCommand::Resize {
                        width,
                        height,
                        scale,
                    },
                });
            }
        }
    }

    /// Whether the person sees the browser now. The shown page follows it.
    pub fn set_visible(&self, visible: bool) {
        let (link, page) = {
            let mut inner = self.lock();
            inner.visible = visible;
            let created = inner
                .shown
                .filter(|page| inner.pages.get(page).is_some_and(|entry| entry.created));
            (inner.link.clone(), created)
        };
        if let (Some(link), Some(page)) = (link, page) {
            let _ = link.send(&ToHost::Page {
                page,
                command: PageCommand::Visible { visible },
            });
        }
    }

    /// A page an agent works on is painted only while a tool runs on it: a hidden page does not
    /// take clicks. The shown page is governed by `set_visible`, not by this.
    pub fn set_page_working(&self, page: PageId, working: bool) {
        let link = {
            let inner = self.lock();
            let created = inner.pages.get(&page).is_some_and(|entry| entry.created);
            if inner.shown == Some(page) || !created {
                return;
            }
            inner.link.clone()
        };
        if let Some(link) = link {
            let _ = link.send(&ToHost::Page {
                page,
                command: PageCommand::Visible { visible: working },
            });
        }
    }

    /// A terminal session's agent started or stopped working. Only the page it is in control of
    /// reacts, so the message goes to every page.
    pub fn session_status(&self, session: &str, working: bool) {
        let pointers: Vec<Arc<AgentPointer>> = self
            .lock()
            .pages
            .values()
            .map(|entry| Arc::clone(&entry.pointer))
            .collect();
        let now = Instant::now();
        for pointer in pointers {
            pointer.session_status(session, working, now);
        }
    }

    /// Records that the shown page is in its own window (or back in the main one).
    pub fn set_detached(&self, detached: bool) {
        let mut inner = self.lock();
        if inner.detached != detached {
            inner.detached = detached;
            self.publish(&mut inner);
        }
    }

    /// Stops the engine. Safe to call when it is not running.
    pub fn shutdown(&self) {
        self.stopping.store(true, Ordering::Release);
        let link = self.lock().link.take();
        // Dropping the last handle stops the process, which can take a moment.
        drop(link);
        let mut inner = self.lock();
        inner.pages.clear();
        inner.agent_pages.clear();
        inner.shown = None;
        inner.engine = BrowserStatus::Stopped;
        inner.engine_error = None;
        self.publish(&mut inner);
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

    #[derive(Default)]
    struct Recorder {
        states: Mutex<Vec<BrowserState>>,
        pages: Mutex<Vec<Vec<BrowserPageInfo>>>,
        thumbnails: Mutex<Vec<(PageId, String)>>,
    }

    impl BrowserEvents for Recorder {
        fn state_changed(&self, state: &BrowserState) {
            self.states.lock().unwrap().push(state.clone());
        }

        fn pages_changed(&self, pages: &[BrowserPageInfo]) {
            self.pages.lock().unwrap().push(pages.to_vec());
        }

        fn thumbnail_changed(&self, page: PageId, png: &str) {
            self.thumbnails.lock().unwrap().push((page, png.to_owned()));
        }
    }

    struct Nowhere;

    impl FrameSink for Nowhere {
        fn frame(&self, _frame: Frame) {}
    }

    fn service(home: &std::path::Path) -> (Arc<BrowserService>, Arc<Recorder>) {
        let recorder = Arc::new(Recorder::default());
        let service = BrowserService::new(
            home.to_path_buf(),
            Arc::clone(&recorder) as Arc<dyn BrowserEvents>,
            Arc::new(Nowhere),
        );
        (Arc::new(service), recorder)
    }

    fn workspace(path: &str) -> PageOwner {
        PageOwner::Workspace {
            worktree_path: path.to_owned(),
        }
    }

    /// Puts a page in the registry as if the engine had made it.
    fn made_page(service: &BrowserService, owner: PageOwner, url: &str) -> PageId {
        let mut inner = service.lock();
        let page = inner.next_page;
        inner.next_page += 1;
        let mut entry = Page::new(owner);
        entry.created = true;
        entry.content.url = url.to_owned();
        inner.pages.insert(page, entry);
        page
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
            recorder.states.lock().unwrap().len(),
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
            recorder.states.lock().unwrap().is_empty(),
            "no engine was looked for"
        );
    }

    #[test]
    fn commands_need_a_running_engine() {
        let home = tempfile::tempdir().unwrap();
        let (service, _) = service(home.path());
        assert!(matches!(
            service.send_shown(PageCommand::Back),
            Err(BrowserError::NotRunning)
        ));
        assert!(matches!(
            service.navigate("example.com"),
            Err(BrowserError::NotRunning)
        ));
        assert!(matches!(
            service.send(7, PageCommand::Back),
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

    #[test]
    fn pages_are_listed_with_their_owner_in_the_order_made() {
        let home = tempfile::tempdir().unwrap();
        let (service, _) = service(home.path());
        let first = made_page(&service, workspace("/a"), "https://a.test/");
        let second = made_page(&service, PageOwner::Bot { id: "b".into() }, "");
        let pages = service.pages();
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].id, first);
        assert_eq!(pages[0].owner, workspace("/a"));
        assert_eq!(pages[0].url, "https://a.test/");
        assert_eq!(pages[1].id, second);
        assert_eq!(pages[1].owner, PageOwner::Bot { id: "b".into() });
    }

    #[test]
    fn showing_a_page_makes_it_the_one_the_person_sees() {
        let home = tempfile::tempdir().unwrap();
        let (service, _) = service(home.path());
        let page = made_page(&service, workspace("/a"), "https://a.test/");
        assert_eq!(service.state().page, None);
        service.lock().shown = Some(page);
        assert!(service.is_shown(page));
        assert_eq!(service.state().page, Some(page));
        assert_eq!(service.state().url, "https://a.test/");
    }

    #[test]
    fn showing_a_page_that_does_not_exist_is_refused() {
        let home = tempfile::tempdir().unwrap();
        let (service, _) = service(home.path());
        assert!(matches!(
            service.show_page(9),
            Err(BrowserError::NotRunning)
        ));
    }

    #[test]
    fn a_page_closed_by_the_engine_is_forgotten_and_no_longer_shown() {
        let home = tempfile::tempdir().unwrap();
        let (service, _) = service(home.path());
        let page = made_page(&service, workspace("/a"), "");
        {
            let mut inner = service.lock();
            inner.agent_pages.insert(workspace("/a"), page);
            inner.shown = Some(page);
        }
        service.on_link_event(LinkEvent::Message(FromHost::PageClosed { page }), None);
        assert!(service.pages().is_empty());
        assert_eq!(service.state().page, None);
        assert!(service.turn_of(page).is_none());
    }

    #[test]
    fn a_thumbnail_is_kept_per_page_and_told_to_the_interface() {
        let home = tempfile::tempdir().unwrap();
        let (service, recorder) = service(home.path());
        let page = made_page(&service, workspace("/a"), "");
        service.store_thumbnail(page, "AAAA");
        assert_eq!(
            service.thumbnails(),
            vec![BrowserThumbnail {
                page,
                png: "AAAA".into()
            }]
        );
        assert_eq!(
            recorder.thumbnails.lock().unwrap().as_slice(),
            &[(page, "AAAA".to_owned())]
        );
    }

    #[test]
    fn the_list_is_told_when_it_changes_and_not_otherwise() {
        let home = tempfile::tempdir().unwrap();
        let (service, recorder) = service(home.path());
        {
            let mut inner = service.lock();
            service.publish(&mut inner);
        }
        assert!(recorder.pages.lock().unwrap().is_empty());
        made_page(&service, workspace("/a"), "");
        {
            let mut inner = service.lock();
            service.publish(&mut inner);
            service.publish(&mut inner);
        }
        assert_eq!(recorder.pages.lock().unwrap().len(), 1);
    }

    // Needs the engine installed under the real Lynx home (`bun run browser:engine`). Run with
    // `cargo test -p lynx-ide the_service_drives_a_real_engine -- --ignored --nocapture`.
    #[test]
    #[ignore = "needs the installed browser engine"]
    fn the_service_drives_a_real_engine() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::atomic::AtomicUsize;

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
        let recorder = Arc::new(Recorder::default());
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
        service
            .new_page(workspace("/test"), Some(&format!("127.0.0.1:{port}")))
            .expect("the engine starts");
        let state = service.state();
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

        service.send_shown(PageCommand::Back).unwrap();
        wait_until("going back", &|| service.state().title == "Hello from Lynx");

        assert!(
            recorder
                .states
                .lock()
                .unwrap()
                .iter()
                .any(|state| state.status == BrowserStatus::Starting),
            "the interface saw the engine start"
        );

        service.shutdown();
        assert_eq!(service.state().status, BrowserStatus::Stopped);
        assert!(matches!(
            service.send_shown(PageCommand::Back),
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
            recorder.states.lock().unwrap().len(),
            1,
            "an unchanged value is not news"
        );
        assert!(service.state().detached);

        // A failed start replaces the engine's status; the window it is shown in must not be lost.
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
