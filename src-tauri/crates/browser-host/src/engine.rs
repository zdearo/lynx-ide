//! The CEF side of the host: the handlers that turn engine callbacks into protocol messages, and
//! the functions that turn protocol commands into engine calls. Everything here runs on the main
//! thread, which is CEF's UI thread, so the state is thread-local.

use std::cell::RefCell;
use std::os::unix::net::UnixStream;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::time::Duration;

use cef::*;
use lynx_browser_protocol::frame_buffer::{FrameBuffer, MAX_HEIGHT, MAX_WIDTH, SlotPool};
use lynx_browser_protocol::message::{FromHost, ToHost, write_message};

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
static BROWSER_CLOSED: AtomicBool = AtomicBool::new(false);

pub fn context_ready() -> bool {
    CONTEXT_READY.load(Ordering::Acquire)
}

pub fn browser_closed() -> bool {
    BROWSER_CLOSED.load(Ordering::Acquire)
}

struct State {
    browser: Option<Browser>,
    width: i32,
    height: i32,
    scale: f32,
    pool: SlotPool,
    frames: Option<FrameBuffer>,
    link: Option<UnixStream>,
    /// Keeps the DevTools observer attached; dropping it would stop the answers.
    devtools: Option<Registration>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            browser: None,
            width: 800,
            height: 600,
            scale: 1.0,
            pool: SlotPool::default(),
            frames: None,
            link: None,
            devtools: None,
        }
    }
}

thread_local! {
    static STATE: RefCell<State> = RefCell::default();
}

/// Never call CEF from inside `read`: the state is borrowed meanwhile.
fn with_state<T>(read: impl FnOnce(&mut State) -> T) -> T {
    STATE.with(|state| read(&mut state.borrow_mut()))
}

/// Gives the engine the way back to Lynx. Call once, before creating the page.
pub fn connect(frames: FrameBuffer, link: UnixStream) {
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

fn browser() -> Option<Browser> {
    with_state(|state| state.browser.clone())
}

fn host() -> Option<BrowserHost> {
    browser().and_then(|browser| browser.host())
}

/// Creates the one page, blank. Call once CEF's context is ready; `Ready` goes out when it exists.
pub fn create_page() {
    let window = WindowInfo::default().set_as_windowless(0);
    let settings = BrowserSettings {
        windowless_frame_rate: 60,
        ..Default::default()
    };
    let mut client = HostClient::new();
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
        eprintln!("lynx-browser-host: the engine refused to create a page ({answer})");
    }
}

/// Applies one command from Lynx.
pub fn handle(command: ToHost) {
    let Some(host) = host() else {
        eprintln!("lynx-browser-host: command before the page was ready: {command:?}");
        return;
    };
    match command {
        ToHost::Navigate { url } => {
            if let Some(frame) = browser().and_then(|browser| browser.main_frame()) {
                frame.load_url(Some(&CefString::from(url.as_str())));
            }
        }
        ToHost::Reload { ignore_cache } => {
            if let Some(browser) = browser() {
                if ignore_cache {
                    browser.reload_ignore_cache();
                } else {
                    browser.reload();
                }
            }
        }
        ToHost::Back => {
            if let Some(browser) = browser() {
                browser.go_back();
            }
        }
        ToHost::Forward => {
            if let Some(browser) = browser() {
                browser.go_forward();
            }
        }
        ToHost::Stop => {
            if let Some(browser) = browser() {
                browser.stop_load();
            }
        }
        ToHost::Resize {
            width,
            height,
            scale,
        } => {
            with_state(|state| {
                state.width = width.max(1) as i32;
                state.height = height.max(1) as i32;
                state.scale = scale.max(1.0);
            });
            host.notify_screen_info_changed();
            host.was_resized();
        }
        ToHost::Visible { visible } => host.was_hidden(i32::from(!visible)),
        ToHost::Focus { focused } => host.set_focus(i32::from(focused)),
        ToHost::FrameDone { slot } => {
            let repaint = with_state(|state| state.pool.release(usize::from(slot)));
            if repaint {
                host.invalidate(PaintElementType::VIEW);
            }
        }
        ToHost::Cdp { id, method, params } => send_cdp(&host, id, &method, params),
        ToHost::Shutdown => {}
        input_command => input::apply(&host, input_command),
    }
}

/// Sends one DevTools call to the page; the answer comes back through [`HostDevTools`].
fn send_cdp(host: &BrowserHost, id: u32, method: &str, params: serde_json::Value) {
    let message = serde_json::json!({ "id": id, "method": method, "params": params });
    let accepted = serde_json::to_vec(&message)
        .is_ok_and(|bytes| host.send_dev_tools_message(Some(&bytes)) == 1);
    if !accepted {
        emit(FromHost::CdpResult {
            id,
            ok: false,
            result: serde_json::json!({ "message": "the page refused the DevTools call" }),
        });
    }
}

/// Closes the page. `browser_closed` turns true once the engine finished doing so.
pub fn close_page() {
    if let Some(host) = with_state(|state| state.browser.take()).and_then(|b| b.host()) {
        host.close_browser(1);
    } else {
        BROWSER_CLOSED.store(true, Ordering::Release);
    }
}

wrap_dev_tools_message_observer! {
    struct HostDevTools {}

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
            emit(FromHost::CdpResult {
                id: message_id.max(0) as u32,
                ok: success == 1,
                result,
            });
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
    struct HostLifeSpan {}

    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut Browser>) {
            if let Some(browser) = browser {
                let browser = browser.clone();
                let registration = browser.host().and_then(|host| {
                    host.add_dev_tools_message_observer(Some(&mut HostDevTools::new()))
                });
                with_state(|state| {
                    state.browser = Some(browser);
                    state.devtools = registration;
                });
                emit(FromHost::Ready);
            }
        }

        fn on_before_close(&self, _browser: Option<&mut Browser>) {
            BROWSER_CLOSED.store(true, Ordering::Release);
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
            }
            1
        }
    }
}

wrap_display_handler! {
    struct HostDisplay {}

    impl DisplayHandler {
        fn on_title_change(&self, _browser: Option<&mut Browser>, title: Option<&CefString>) {
            if let Some(title) = title {
                emit(FromHost::Title { title: title.to_string() });
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
                emit(FromHost::Address { url: url.to_string() });
            }
        }
    }
}

wrap_load_handler! {
    struct HostLoad {}

    impl LoadHandler {
        fn on_loading_state_change(
            &self,
            _browser: Option<&mut Browser>,
            is_loading: std::os::raw::c_int,
            can_go_back: std::os::raw::c_int,
            can_go_forward: std::os::raw::c_int,
        ) {
            emit(FromHost::Loading {
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
            emit(FromHost::LoadFailed {
                url,
                message: format!("{error_code:?} {text}").trim().to_owned(),
            });
        }
    }
}

wrap_render_handler! {
    struct HostRender {}

    impl RenderHandler {
        fn view_rect(&self, _browser: Option<&mut Browser>, rect: Option<&mut Rect>) {
            let (width, height) = with_state(|state| (state.width, state.height));
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
            let (width, height, scale) =
                with_state(|state| (state.width, state.height, state.scale));
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
            let length = width as usize * height as usize * 4;
            // SAFETY: CEF passes a BGRA buffer of width * height * 4 bytes, valid only during
            // this call; it is copied into shared memory before returning.
            let pixels = unsafe { std::slice::from_raw_parts(buffer, length) };
            let slot = with_state(|state| {
                let slot = state.pool.acquire()?;
                let frames = state.frames.as_mut()?;
                match frames.write(slot, pixels) {
                    Ok(()) => Some(slot),
                    Err(error) => {
                        eprintln!("lynx-browser-host: frame not written: {error}");
                        state.pool.release(slot);
                        None
                    }
                }
            });
            // With no free slot the frame is dropped; `FrameDone` brings a repaint.
            if let Some(slot) = slot {
                emit(FromHost::Frame {
                    slot: slot as u8,
                    width: width as u32,
                    height: height as u32,
                });
            }
        }
    }
}

wrap_client! {
    struct HostClient {}

    impl Client {
        fn render_handler(&self) -> Option<RenderHandler> {
            Some(HostRender::new())
        }

        fn display_handler(&self) -> Option<DisplayHandler> {
            Some(HostDisplay::new())
        }

        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(HostLifeSpan::new())
        }

        fn load_handler(&self) -> Option<LoadHandler> {
            Some(HostLoad::new())
        }
    }
}
