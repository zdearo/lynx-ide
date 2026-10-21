//! The CEF side of the host: the handlers that turn engine callbacks into protocol messages, and
//! the functions that turn protocol commands into engine calls. Everything here runs on the main
//! thread, which is CEF's UI thread, so the state is thread-local.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::time::Duration;

use cef::*;
use lynx_browser_protocol::frame_buffer::{FrameBuffer, MAX_HEIGHT, MAX_WIDTH, SlotPool};
use lynx_browser_protocol::message::{
    EditCommand, FromHost, NoticeKind, PageCommand, PageEvent, PageId, ToHost, write_message,
};
use lynx_browser_protocol::transport::Connection;

use crate::input;

/// What wakes the main loop.
pub enum Event {
    Command(ToHost),
    /// The engine wants a message-loop pass after this delay. It may ask from any thread.
    Pump(Duration),
    /// Lynx closed the connection, so there is nobody left to serve.
    LinkClosed,
}

/// Set once, before CEF starts, so callbacks on any thread can reach the main loop.
pub static EVENTS: OnceLock<Sender<Event>> = OnceLock::new();
static CONTEXT_READY: AtomicBool = AtomicBool::new(false);

pub fn context_ready() -> bool {
    CONTEXT_READY.load(Ordering::Acquire)
}

/// One page: a browser the engine draws off-screen, and what it needs to draw it.
struct Page {
    /// None until the engine has made the browser, and again once it is being closed.
    browser: Option<Browser>,
    closing: bool,
    width: i32,
    height: i32,
    scale: f32,
    /// Keeps the DevTools observer attached; dropping it would stop the answers.
    devtools: Option<Registration>,
}

impl Default for Page {
    fn default() -> Self {
        Self {
            browser: None,
            closing: false,
            width: 800,
            height: 600,
            scale: 1.0,
            devtools: None,
        }
    }
}

#[derive(Default)]
struct State {
    pages: HashMap<PageId, Page>,
    /// The engine's own id for each browser, so a callback that only names the browser finds its page.
    by_browser: HashMap<i32, PageId>,
    /// The frame slots are shared by every page.
    pool: SlotPool,
    /// Pages whose frame was dropped for lack of a free slot; they paint again once one is back.
    starved: HashSet<PageId>,
    frames: Option<FrameBuffer>,
    link: Option<Connection>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::default();
}

/// Never call CEF from inside `read`: the state is borrowed meanwhile.
fn with_state<T>(read: impl FnOnce(&mut State) -> T) -> T {
    STATE.with(|state| read(&mut state.borrow_mut()))
}

/// Gives the engine the way back to Lynx. Call once, before telling Lynx it is ready.
pub fn connect(frames: FrameBuffer, link: Connection) {
    with_state(|state| {
        state.frames = Some(frames);
        state.link = Some(link);
    });
}

fn emit(message: FromHost) {
    with_state(|state| {
        if let Some(link) = state.link.as_mut()
            && let Err(error) = write_message(link, &message)
        {
            eprintln!("lynx-browser-host: could not tell Lynx: {error}");
        }
    });
}

fn emit_page(page: PageId, event: PageEvent) {
    emit(FromHost::Page { page, event });
}

/// Tells Lynx the engine is up. Pages are made when Lynx asks for them.
pub fn announce_ready() {
    emit(FromHost::Ready);
}

fn browser_of(page: PageId) -> Option<Browser> {
    with_state(|state| state.pages.get(&page).and_then(|page| page.browser.clone()))
}

/// Makes one blank page. `PageCreated` goes out once the engine has it.
fn create_page(page: PageId) {
    if with_state(|state| state.pages.contains_key(&page)) {
        eprintln!("lynx-browser-host: page {page} already exists");
        return;
    }
    with_state(|state| state.pages.insert(page, Page::default()));
    let window = WindowInfo::default().set_as_windowless(Default::default());
    let settings = BrowserSettings {
        windowless_frame_rate: 60,
        ..Default::default()
    };
    let mut client = HostClient::new(page);
    let url = CefString::from("about:blank");
    let answer = browser_host_create_browser(
        Some(&window),
        Some(&mut client),
        Some(&url),
        Some(&settings),
        None,
        None,
    );
    if answer != 1 {
        eprintln!("lynx-browser-host: the engine refused to create page {page} ({answer})");
        with_state(|state| state.pages.remove(&page));
        emit(FromHost::PageClosed { page });
    }
}

/// Closes a page. `PageClosed` goes out once the engine finished doing so.
fn close_page(page: PageId) {
    enum Step {
        Unknown,
        Closing,
        NotMadeYet,
        Close(Browser),
    }
    let step = with_state(|state| match state.pages.get_mut(&page) {
        None => Step::Unknown,
        Some(page) if page.closing => Step::Closing,
        Some(page) => match page.browser.take() {
            Some(browser) => {
                page.closing = true;
                Step::Close(browser)
            }
            None => Step::NotMadeYet,
        },
    });
    match step {
        Step::Closing => {}
        // Closing a page that is already gone is not an error; Lynx just wants to hear it is gone.
        Step::Unknown => emit(FromHost::PageClosed { page }),
        // The browser is still being made: it closes itself when `on_after_created` finds no page.
        Step::NotMadeYet => {
            with_state(|state| state.pages.remove(&page));
            emit(FromHost::PageClosed { page });
        }
        Step::Close(browser) => {
            if let Some(host) = browser.host() {
                host.close_browser(1);
            }
        }
    }
}

/// Asks every page to close, for when the host quits.
pub fn close_all_pages() {
    let pages: Vec<PageId> = with_state(|state| state.pages.keys().copied().collect());
    for page in pages {
        close_page(page);
    }
}

/// True once no page is left, which is when `close_all_pages` is done.
pub fn pages_closed() -> bool {
    with_state(|state| state.pages.is_empty())
}

/// Applies one command from Lynx.
pub fn handle(command: ToHost) {
    match command {
        ToHost::CreatePage { page } => create_page(page),
        ToHost::ClosePage { page } => close_page(page),
        ToHost::FrameDone { slot } => release_slot(slot),
        ToHost::Page { page, command } => handle_page(page, command),
        ToHost::Shutdown => {}
    }
}

/// A slot is free again, so pages that went without a frame paint once more.
fn release_slot(slot: u8) {
    let starved: Vec<PageId> = with_state(|state| {
        if state.pool.release(usize::from(slot)) {
            state.starved.drain().collect()
        } else {
            Vec::new()
        }
    });
    for page in starved {
        if let Some(host) = browser_of(page).and_then(|browser| browser.host()) {
            host.invalidate(PaintElementType::VIEW);
        }
    }
}

fn handle_page(page: PageId, command: PageCommand) {
    let Some(browser) = browser_of(page) else {
        eprintln!("lynx-browser-host: page {page} cannot take a command: {command:?}");
        return;
    };
    let Some(host) = browser.host() else { return };
    match command {
        PageCommand::Navigate { url } => {
            if let Some(frame) = browser.main_frame() {
                frame.load_url(Some(&CefString::from(url.as_str())));
            }
        }
        PageCommand::Reload { ignore_cache } => {
            if ignore_cache {
                browser.reload_ignore_cache();
            } else {
                browser.reload();
            }
        }
        PageCommand::Back => browser.go_back(),
        PageCommand::Forward => browser.go_forward(),
        PageCommand::Stop => browser.stop_load(),
        PageCommand::Resize {
            width,
            height,
            scale,
        } => {
            with_state(|state| {
                if let Some(page) = state.pages.get_mut(&page) {
                    page.width = width.max(1) as i32;
                    page.height = height.max(1) as i32;
                    page.scale = scale.max(1.0);
                }
            });
            host.notify_screen_info_changed();
            host.was_resized();
        }
        PageCommand::Visible { visible } => host.was_hidden(i32::from(!visible)),
        PageCommand::Focus { focused } => host.set_focus(i32::from(focused)),
        PageCommand::Cdp { id, method, params } => send_cdp(page, &host, id, &method, params),
        PageCommand::Edit { command } => edit(&browser, command),
        input_command => input::apply(&host, input_command),
    }
}

/// Runs an editing action on the frame that has the focus, falling back to the main frame.
fn edit(browser: &Browser, command: EditCommand) {
    let Some(frame) = browser.focused_frame().or_else(|| browser.main_frame()) else {
        return;
    };
    match command {
        EditCommand::Undo => frame.undo(),
        EditCommand::Redo => frame.redo(),
        EditCommand::Cut => frame.cut(),
        EditCommand::Copy => frame.copy(),
        EditCommand::Paste => frame.paste(),
        EditCommand::SelectAll => frame.select_all(),
    }
}

/// Sends one DevTools call to the page; the answer comes back through [`HostDevTools`].
fn send_cdp(page: PageId, host: &BrowserHost, id: u32, method: &str, params: serde_json::Value) {
    let message = serde_json::json!({ "id": id, "method": method, "params": params });
    let accepted = serde_json::to_vec(&message)
        .is_ok_and(|bytes| host.send_dev_tools_message(Some(&bytes)) == 1);
    if !accepted {
        emit_page(
            page,
            PageEvent::CdpResult {
                id,
                ok: false,
                result: serde_json::json!({ "message": "the page refused the DevTools call" }),
            },
        );
    }
}

wrap_dev_tools_message_observer! {
    struct HostDevTools {
        page: PageId,
    }

    impl DevToolsMessageObserver {
        fn on_dev_tools_method_result(
            &self,
            _browser: Option<&mut Browser>,
            message_id: std::os::raw::c_int,
            success: std::os::raw::c_int,
            result: Option<&[u8]>,
        ) {
            let result = result
                .and_then(|bytes| serde_json::from_slice(bytes).ok())
                .unwrap_or(serde_json::Value::Null);
            emit_page(self.page, PageEvent::CdpResult {
                id: message_id.max(0) as u32,
                ok: success == 1,
                result,
            });
        }

        fn on_dev_tools_event(
            &self,
            _browser: Option<&mut Browser>,
            method: Option<&CefString>,
            params: Option<&[u8]>,
        ) {
            let Some(method) = method.map(|method| method.to_string()) else {
                return;
            };
            // Network and runtime events are many; only the few Lynx reads cross the socket.
            if !FORWARDED_EVENTS.contains(&method.as_str()) {
                return;
            }
            let params = params
                .and_then(|bytes| serde_json::from_slice(bytes).ok())
                .unwrap_or(serde_json::Value::Null);
            emit_page(self.page, PageEvent::CdpEvent { method, params });
        }

        fn on_dev_tools_agent_attached(&self, _browser: Option<&mut Browser>) {
            emit_page(self.page, PageEvent::DevToolsAttached);
        }
    }
}

/// The DevTools events that are passed on to Lynx.
const FORWARDED_EVENTS: &[&str] = &[
    "Runtime.consoleAPICalled",
    "Runtime.exceptionThrown",
    "Log.entryAdded",
    "Network.requestWillBeSent",
    "Network.responseReceived",
    "Network.loadingFailed",
    "Page.frameNavigated",
];

/// Tells Lynx what the engine answered on its own, in words.
fn notice(page: PageId, kind: NoticeKind, text: String) {
    emit_page(page, PageEvent::Notice { kind, text });
}

/// A quoted, shortened copy of what a page wrote, for a notice.
fn quoted(text: Option<&CefString>) -> String {
    let text = text.map(|text| text.to_string()).unwrap_or_default();
    let mut short: String = text.chars().take(120).collect();
    if text.chars().count() > 120 {
        short.push('…');
    }
    format!("\"{short}\"")
}

wrap_jsdialog_handler! {
    struct HostJsDialog {
        page: PageId,
    }

    impl JsdialogHandler {
        // Nobody sits at this page to answer a dialog, and a page waiting on one would hang every
        // call after it. An alert is read and closed; a question gets the safe answer, no.
        fn on_jsdialog(
            &self,
            _browser: Option<&mut Browser>,
            _origin_url: Option<&CefString>,
            dialog_type: JsdialogType,
            message_text: Option<&CefString>,
            _default_prompt_text: Option<&CefString>,
            callback: Option<&mut JsdialogCallback>,
            _suppress_message: Option<&mut std::os::raw::c_int>,
        ) -> std::os::raw::c_int {
            let message = quoted(message_text);
            let (kind, accept) = if dialog_type == JsdialogType::ALERT {
                ("alert", true)
            } else if dialog_type == JsdialogType::CONFIRM {
                ("confirm", false)
            } else {
                ("prompt", false)
            };
            if let Some(callback) = callback {
                callback.cont(i32::from(accept), None);
            }
            let answer = if accept { "closed" } else { "answered no" };
            notice(
                self.page,
                NoticeKind::Dialog,
                format!("The page showed a {kind} {message}; Lynx {answer}."),
            );
            1
        }

        // Leaving a page is the agent's choice, so a "leave this page?" prompt lets it leave.
        fn on_before_unload_dialog(
            &self,
            _browser: Option<&mut Browser>,
            message_text: Option<&CefString>,
            _is_reload: std::os::raw::c_int,
            callback: Option<&mut JsdialogCallback>,
        ) -> std::os::raw::c_int {
            if let Some(callback) = callback {
                callback.cont(1, None);
            }
            notice(
                self.page,
                NoticeKind::Dialog,
                format!("The page asked to confirm leaving {}; Lynx let it leave.", quoted(message_text)),
            );
            1
        }
    }
}

wrap_permission_handler! {
    struct HostPermissions {
        page: PageId,
    }

    impl PermissionHandler {
        fn on_request_media_access_permission(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            requesting_origin: Option<&CefString>,
            _requested_permissions: u32,
            callback: Option<&mut MediaAccessCallback>,
        ) -> std::os::raw::c_int {
            if let Some(callback) = callback {
                callback.cancel();
            }
            notice(
                self.page,
                NoticeKind::Permission,
                format!("{} asked for the camera or microphone; Lynx refused.", quoted(requesting_origin)),
            );
            1
        }

        fn on_show_permission_prompt(
            &self,
            _browser: Option<&mut Browser>,
            _prompt_id: u64,
            requesting_origin: Option<&CefString>,
            _requested_permissions: u32,
            callback: Option<&mut PermissionPromptCallback>,
        ) -> std::os::raw::c_int {
            if let Some(callback) = callback {
                callback.cont(PermissionRequestResult::DENY);
            }
            notice(
                self.page,
                NoticeKind::Permission,
                format!("{} asked for a permission; Lynx refused.", quoted(requesting_origin)),
            );
            1
        }
    }
}

wrap_download_handler! {
    struct HostDownloads {
        page: PageId,
    }

    impl DownloadHandler {
        // Nothing is saved to the person's disk on a page's say-so.
        fn can_download(
            &self,
            _browser: Option<&mut Browser>,
            url: Option<&CefString>,
            _request_method: Option<&CefString>,
        ) -> std::os::raw::c_int {
            notice(
                self.page,
                NoticeKind::Download,
                format!("A download from {} was blocked.", quoted(url)),
            );
            0
        }
    }
}

wrap_dialog_handler! {
    struct HostFileChooser {
        page: PageId,
    }

    impl DialogHandler {
        fn on_file_dialog(
            &self,
            _browser: Option<&mut Browser>,
            _mode: FileDialogMode,
            _title: Option<&CefString>,
            _default_file_path: Option<&CefString>,
            _accept_filters: Option<&mut CefStringList>,
            _accept_extensions: Option<&mut CefStringList>,
            _accept_descriptions: Option<&mut CefStringList>,
            callback: Option<&mut FileDialogCallback>,
        ) -> std::os::raw::c_int {
            if let Some(callback) = callback {
                callback.cancel();
            }
            notice(
                self.page,
                NoticeKind::FileChooser,
                "The page asked for a file; no file chooser is offered.".to_owned(),
            );
            1
        }
    }
}

wrap_context_menu_handler! {
    struct HostContextMenu {}

    impl ContextMenuHandler {
        // The native menu has nowhere to be drawn in an off-screen page.
        fn on_before_context_menu(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _params: Option<&mut ContextMenuParams>,
            model: Option<&mut MenuModel>,
        ) {
            if let Some(model) = model {
                model.clear();
            }
        }
    }
}

wrap_app! {
    pub struct HostApp {}

    impl App {
        fn browser_process_handler(&self) -> Option<BrowserProcessHandler> {
            Some(HostBrowserProcess::new())
        }
    }
}

wrap_browser_process_handler! {
    struct HostBrowserProcess {}

    impl BrowserProcessHandler {
        fn on_context_initialized(&self) {
            CONTEXT_READY.store(true, Ordering::Release);
        }

        fn on_schedule_message_pump_work(&self, delay_ms: i64) {
            if let Some(events) = EVENTS.get() {
                let delay = Duration::from_millis(delay_ms.max(0) as u64);
                let _ = events.send(Event::Pump(delay));
            }
        }
    }
}

wrap_life_span_handler! {
    struct HostLifeSpan {
        page: PageId,
    }

    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut Browser>) {
            let Some(browser) = browser else { return };
            let browser = browser.clone();
            let page = self.page;
            // Lynx closed the page while the engine was still making it.
            if !with_state(|state| state.pages.contains_key(&page)) {
                if let Some(host) = browser.host() {
                    host.close_browser(1);
                }
                return;
            }
            let registration = browser.host().and_then(|host| {
                host.add_dev_tools_message_observer(Some(&mut HostDevTools::new(page)))
            });
            let identifier = browser.identifier();
            with_state(|state| {
                state.by_browser.insert(identifier, page);
                if let Some(entry) = state.pages.get_mut(&page) {
                    entry.browser = Some(browser);
                    entry.devtools = registration;
                }
            });
            emit(FromHost::PageCreated { page });
        }

        fn on_before_close(&self, browser: Option<&mut Browser>) {
            let identifier = browser.map(|browser| browser.identifier());
            let page = with_state(|state| {
                let page = identifier
                    .and_then(|identifier| state.by_browser.remove(&identifier))
                    .unwrap_or(self.page);
                state.starved.remove(&page);
                state.pages.remove(&page).map(|_| page)
            });
            if let Some(page) = page {
                emit(FromHost::PageClosed { page });
            }
        }

        // A window that would pop up has nowhere to be shown: load it in the page instead.
        fn on_before_popup(
            &self,
            browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _popup_id: std::os::raw::c_int,
            target_url: Option<&CefString>,
            _target_frame_name: Option<&CefString>,
            _target_disposition: WindowOpenDisposition,
            _user_gesture: std::os::raw::c_int,
            _popup_features: Option<&PopupFeatures>,
            _window_info: Option<&mut WindowInfo>,
            _client: Option<&mut Option<Client>>,
            _settings: Option<&mut BrowserSettings>,
            _extra_info: Option<&mut Option<DictionaryValue>>,
            _no_javascript_access: Option<&mut std::os::raw::c_int>,
        ) -> std::os::raw::c_int {
            if let (Some(browser), Some(url)) = (browser, target_url)
                && let Some(frame) = browser.main_frame()
            {
                frame.load_url(Some(url));
                notice(
                    self.page,
                    NoticeKind::Popup,
                    format!("A new window to {} opened in this page.", quoted(Some(url))),
                );
            }
            1
        }
    }
}

wrap_display_handler! {
    struct HostDisplay {
        page: PageId,
    }

    impl DisplayHandler {
        fn on_title_change(&self, _browser: Option<&mut Browser>, title: Option<&CefString>) {
            if let Some(title) = title {
                emit_page(self.page, PageEvent::Title { title: title.to_string() });
            }
        }

        fn on_address_change(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            url: Option<&CefString>,
        ) {
            let main = frame.is_some_and(|frame| frame.is_main() == 1);
            if let (true, Some(url)) = (main, url) {
                emit_page(self.page, PageEvent::Address { url: url.to_string() });
            }
        }
    }
}

wrap_load_handler! {
    struct HostLoad {
        page: PageId,
    }

    impl LoadHandler {
        fn on_loading_state_change(
            &self,
            _browser: Option<&mut Browser>,
            is_loading: std::os::raw::c_int,
            can_go_back: std::os::raw::c_int,
            can_go_forward: std::os::raw::c_int,
        ) {
            emit_page(self.page, PageEvent::Loading {
                loading: is_loading == 1,
                can_go_back: can_go_back == 1,
                can_go_forward: can_go_forward == 1,
            });
        }

        fn on_load_error(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            error_code: Errorcode,
            error_text: Option<&CefString>,
            failed_url: Option<&CefString>,
        ) {
            let text = error_text.map(|text| text.to_string()).unwrap_or_default();
            let url = failed_url.map(|url| url.to_string()).unwrap_or_default();
            emit_page(self.page, PageEvent::LoadFailed {
                url,
                message: format!("{error_code:?} {text}").trim().to_owned(),
            });
        }
    }
}

wrap_render_handler! {
    struct HostRender {
        page: PageId,
    }

    impl RenderHandler {
        fn view_rect(&self, _browser: Option<&mut Browser>, rect: Option<&mut Rect>) {
            let (width, height) = with_state(|state| {
                state
                    .pages
                    .get(&self.page)
                    .map_or((800, 600), |page| (page.width, page.height))
            });
            if let Some(rect) = rect {
                *rect = Rect { x: 0, y: 0, width, height };
            }
        }

        fn screen_info(
            &self,
            _browser: Option<&mut Browser>,
            screen_info: Option<&mut ScreenInfo>,
        ) -> std::os::raw::c_int {
            let Some(info) = screen_info else {
                return 0;
            };
            let (width, height, scale) = with_state(|state| {
                state
                    .pages
                    .get(&self.page)
                    .map_or((800, 600, 1.0), |page| (page.width, page.height, page.scale))
            });
            info.device_scale_factor = scale;
            info.rect = Rect { x: 0, y: 0, width, height };
            info.available_rect = Rect { x: 0, y: 0, width, height };
            1
        }

        fn on_paint(
            &self,
            _browser: Option<&mut Browser>,
            type_: PaintElementType,
            _dirty_rects: Option<&[Rect]>,
            buffer: *const u8,
            width: i32,
            height: i32,
        ) {
            // Popup widgets (an open <select>) are not composited into the view yet.
            if type_ != PaintElementType::VIEW || buffer.is_null() || width <= 0 || height <= 0 {
                return;
            }
            if width as u32 > MAX_WIDTH || height as u32 > MAX_HEIGHT {
                eprintln!("lynx-browser-host: a {width}x{height} view is larger than a frame slot");
                return;
            }
            let page = self.page;
            let length = width as usize * height as usize * 4;
            // SAFETY: CEF passes a BGRA buffer of width * height * 4 bytes, valid only during
            // this call; it is copied into shared memory before returning.
            let pixels = unsafe { std::slice::from_raw_parts(buffer, length) };
            let slot = with_state(|state| {
                let Some(slot) = state.pool.acquire() else {
                    // With no free slot the frame is dropped; `FrameDone` brings a repaint.
                    state.starved.insert(page);
                    return None;
                };
                let Some(frames) = state.frames.as_mut() else {
                    state.pool.release(slot);
                    return None;
                };
                match frames.write(slot, pixels) {
                    Ok(()) => Some(slot),
                    Err(error) => {
                        eprintln!("lynx-browser-host: frame not written: {error}");
                        state.pool.release(slot);
                        None
                    }
                }
            });
            if let Some(slot) = slot {
                emit_page(page, PageEvent::Frame {
                    slot: slot as u8,
                    width: width as u32,
                    height: height as u32,
                });
            }
        }
    }
}

wrap_client! {
    struct HostClient {
        page: PageId,
    }

    impl Client {
        fn render_handler(&self) -> Option<RenderHandler> {
            Some(HostRender::new(self.page))
        }

        fn display_handler(&self) -> Option<DisplayHandler> {
            Some(HostDisplay::new(self.page))
        }

        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(HostLifeSpan::new(self.page))
        }

        fn load_handler(&self) -> Option<LoadHandler> {
            Some(HostLoad::new(self.page))
        }

        fn jsdialog_handler(&self) -> Option<JsdialogHandler> {
            Some(HostJsDialog::new(self.page))
        }

        fn permission_handler(&self) -> Option<PermissionHandler> {
            Some(HostPermissions::new(self.page))
        }

        fn download_handler(&self) -> Option<DownloadHandler> {
            Some(HostDownloads::new(self.page))
        }

        fn dialog_handler(&self) -> Option<DialogHandler> {
            Some(HostFileChooser::new(self.page))
        }

        fn context_menu_handler(&self) -> Option<ContextMenuHandler> {
            Some(HostContextMenu::new())
        }
    }
}
