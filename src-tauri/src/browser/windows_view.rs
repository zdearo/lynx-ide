//! The native view of the page on Windows: a child window laid over the webview, in the same
//! window. Frames from the engine are blitted into it, and the mouse and keyboard on it go to the
//! engine. It is a sibling of the WebView2 window and is kept above it, the way Tauri's own child
//! webviews are.
//!
//! Window handles belong to the thread that made them; everything here runs on the main thread.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use lynx_browser_protocol::message::{
    EditCommand, KeyKind, MouseButton, PageCommand, PageId, modifiers,
};
use tauri::{AppHandle, Manager};
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BeginPaint, CreateSolidBrush, DIB_RGB_COLORS,
    DeleteObject, EndPaint, FillRect, HALFTONE, InvalidateRect, PAINTSTRUCT, SRCCOPY,
    ScreenToClient, SetStretchBltMode, StretchDIBits,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::Ime::{
    CFS_POINT, COMPOSITIONFORM, ImmGetContext, ImmReleaseContext, ImmSetCompositionWindow,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, ReleaseCapture, SetCapture, SetFocus, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
    VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CREATESTRUCTW, CreateWindowExW, DLGC_WANTALLKEYS, DefWindowProcW, DestroyWindow, GWLP_USERDATA,
    GetClientRect, GetMessageTime, GetWindowLongPtrW, HTCLIENT, HWND_TOP, IDC_ARROW, KillTimer,
    LoadCursorW, RegisterClassExW, SW_HIDE, SWP_NOACTIVATE, SWP_SHOWWINDOW, SetCursor, SetTimer,
    SetWindowLongPtrW, SetWindowPos, ShowWindow, WM_CHAR, WM_ERASEBKGND, WM_GETDLGCODE,
    WM_IME_STARTCOMPOSITION, WM_KEYDOWN, WM_KEYUP, WM_KILLFOCUS, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCCREATE,
    WM_NCDESTROY, WM_PAINT, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SETCURSOR, WM_SETFOCUS, WM_TIMER,
    WNDCLASSEXW, WS_CHILD, WS_CLIPSIBLINGS, WS_VISIBLE,
};
use windows::core::{PCWSTR, w};

use super::agent_pointer::AgentPointer;
use super::browser_service::{BrowserService, Frame, FrameSink};
use super::click_counter::ClickCounter;
use super::detached_window;
use super::overlay_renderer::{self, OverlayFont};

/// One notch of the wheel, in the units Windows reports.
const WHEEL_NOTCH: i32 = 120;
/// Pixels one notch scrolls, as on the other platforms.
const WHEEL_STEP: i32 = 100;
const FRAME_INTERVAL_MS: u32 = 16;
const ANIMATION_TIMER: usize = 1;
/// Sent once after the mouse leaves a window that asked to be told (`TrackMouseEvent`).
const WM_MOUSELEAVE: u32 = 0x02A3;
const CLASS_NAME: PCWSTR = w!("LynxBrowserView");

thread_local! {
    // One window per Tauri window that has a view. Each view shows one page, or none.
    static VIEWS: RefCell<HashMap<String, HWND>> = RefCell::default();
    // The page each window's view shows, and the agent pointer that page draws over itself.
    static BINDINGS: RefCell<HashMap<String, Binding>> = RefCell::default();
    // The newest frame of each page, drawn by every window that shows the page.
    static FRAMES: RefCell<HashMap<PageId, Frame>> = RefCell::default();
    static CLASS_REGISTERED: Cell<bool> = const { Cell::new(false) };
}

/// What one window's view shows.
#[derive(Clone)]
struct Binding {
    page: PageId,
    pointer: Option<Arc<AgentPointer>>,
}

/// What a window knows about itself, reached through its user data from the window procedure.
struct ViewData {
    service: Arc<BrowserService>,
    /// The window this view is in; its binding says which page it shows.
    label: String,
    clicks: ClickCounter,
    /// Pixels per logical pixel, set each time the view is placed.
    scale: f64,
    /// Windows has been asked to say when the mouse leaves.
    tracking_mouse: bool,
    /// The first half of a character outside the basic plane, waiting for its second.
    high_surrogate: Option<u16>,
    last_position: (i32, i32),
    /// This window's animation timer is running.
    timer_armed: bool,
}

/// The font for labels, read once from the system.
fn font() -> Option<&'static OverlayFont> {
    static FONT: OnceLock<Option<OverlayFont>> = OnceLock::new();
    FONT.get_or_init(OverlayFont::load_system).as_ref()
}

/// Seconds since the first call, for the ring that turns on its own.
fn clock() -> f64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs_f64()
}

struct Shared {
    latest: Mutex<HashMap<PageId, Frame>>,
    /// A paint is already queued on the main thread; newer frames just replace the latest one.
    queued: AtomicBool,
}

/// Where the engine's frames arrive: coalesced, so a slow main thread never builds a backlog.
pub struct NativeView {
    app: AppHandle,
    shared: Arc<Shared>,
}

impl NativeView {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            shared: Arc::new(Shared {
                latest: Mutex::new(HashMap::new()),
                queued: AtomicBool::new(false),
            }),
        }
    }
}

impl FrameSink for NativeView {
    fn frame(&self, page: PageId, frame: Frame) {
        self.shared
            .latest
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(page, frame);
        if self.shared.queued.swap(true, Ordering::AcqRel) {
            return;
        }
        let shared = Arc::clone(&self.shared);
        let _ = self.app.run_on_main_thread(move || {
            shared.queued.store(false, Ordering::Release);
            let frames: Vec<(PageId, Frame)> = shared
                .latest
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .drain()
                .collect();
            for (page, frame) in frames {
                show_frame(page, frame);
            }
        });
    }

    fn pointer_changed(&self) {
        let _ = self.app.run_on_main_thread(animate_pointer);
    }
}

/// Every window that has a view.
fn all_windows() -> Vec<HWND> {
    VIEWS.with(|views| views.borrow().values().copied().collect())
}

/// The windows that show `page`, in whichever window they are.
fn windows_showing(page: PageId) -> Vec<HWND> {
    let labels: Vec<String> = BINDINGS.with(|bindings| {
        bindings
            .borrow()
            .iter()
            .filter(|(_, binding)| binding.page == page)
            .map(|(label, _)| label.clone())
            .collect()
    });
    VIEWS.with(|views| {
        let views = views.borrow();
        labels
            .iter()
            .filter_map(|label| views.get(label).copied())
            .collect()
    })
}

fn invalidate(hwnd: HWND) {
    // SAFETY: the handle is one of ours, on the thread that made it.
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

/// Repaints the windows that show `page`.
fn repaint_page(page: PageId) {
    for hwnd in windows_showing(page) {
        invalidate(hwnd);
    }
}

/// Stores the newest frame of `page` and repaints the windows that show it.
fn show_frame(page: PageId, frame: Frame) {
    FRAMES.with(|frames| frames.borrow_mut().insert(page, frame));
    repaint_page(page);
}

/// Repaints every window for as long as an agent is in control, so the pointer glides and the
/// marks fade. Each window's timer stops itself once no pointer is in control.
fn animate_pointer() {
    for hwnd in all_windows() {
        invalidate(hwnd);
        arm_timer(hwnd);
    }
}

/// Starts the animation timer of one window, unless it runs already.
fn arm_timer(hwnd: HWND) {
    let Some(data) = view_data(hwnd) else {
        return;
    };
    if data.timer_armed {
        return;
    }
    data.timer_armed = true;
    // SAFETY: the handle is one of ours, on the thread that made it.
    unsafe {
        SetTimer(Some(hwnd), ANIMATION_TIMER, FRAME_INTERVAL_MS, None);
    }
}

/// The page the window `label` shows, if it shows one.
fn bound_page(label: &str) -> Option<PageId> {
    BINDINGS.with(|bindings| bindings.borrow().get(label).map(|binding| binding.page))
}

/// Whether a pointer of any page on the screen is in control.
fn control_is_active() -> bool {
    let now = Instant::now();
    BINDINGS.with(|bindings| {
        bindings.borrow().values().any(|binding| {
            binding
                .pointer
                .as_ref()
                .is_some_and(|pointer| pointer.view(now).is_some())
        })
    })
}

/// Puts the view over the page at `rect` in the window `label` (logical pixels, from the
/// webview's top-left corner), creating it first if needed, and tells the engine the size to
/// render the page at. Each window shows one page; a page in its own window is drawn there.
/// Main thread only.
pub fn place(
    app: &AppHandle,
    service: &Arc<BrowserService>,
    label: &str,
    page: PageId,
    rect: (f64, f64, f64, f64),
) -> Result<(), String> {
    let window = app
        .get_webview_window(label)
        .ok_or_else(|| format!("there is no window {label}"))?;
    let scale = window.scale_factor().map_err(|error| error.to_string())?;
    let hwnd = ensure_installed(&window, service, label)?;

    let (x, y, width, height) = rect;
    let (width, height) = (width.round().max(1.0), height.round().max(1.0));
    set_scale(hwnd, scale);
    // SAFETY: the handle is one of ours, on the thread that made it.
    unsafe {
        // On top of its siblings, which include the window WebView2 draws in.
        SetWindowPos(
            hwnd,
            Some(HWND_TOP),
            (x * scale).round() as i32,
            (y * scale).round() as i32,
            (width * scale).round() as i32,
            (height * scale).round() as i32,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        )
        .map_err(|error| error.to_string())?;
    }
    service.resize_page(page, width as u32, height as u32, scale as f32);
    // Read before the borrow, so no call into the service runs while the bindings are borrowed.
    let pointer = service.pointer_of(page);
    BINDINGS.with(|bindings| {
        bindings
            .borrow_mut()
            .insert(label.to_owned(), Binding { page, pointer })
    });
    invalidate(hwnd);
    if control_is_active() {
        arm_timer(hwnd);
    }
    // The main view shows the page the person chose; a page in its own window is always painted.
    if detached_window::is_main_window(label) && !service.is_detached(page) {
        service.set_visible(true);
    }
    Ok(())
}

/// Takes the view off the window `label`. When that window was showing the page, the engine is
/// told nobody is looking, unless the page is in a window of its own. Main thread only.
pub fn hide(service: &Arc<BrowserService>, label: &str) {
    if let Some(hwnd) = VIEWS.with(|views| views.borrow().get(label).copied()) {
        // SAFETY: the handle is one of ours, on the thread that made it.
        unsafe {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
    }
    release(service, label);
}

/// The window `label` is gone, and its view with it. Main thread only.
pub fn forget(service: &Arc<BrowserService>, label: &str) {
    if let Some(hwnd) = VIEWS.with(|views| views.borrow_mut().remove(label)) {
        // SAFETY: the handle is one of ours, on the thread that made it. It may be gone already
        // with its parent, in which case this fails and there is nothing left to do.
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
    }
    release(service, label);
}

/// The window `label` shows nothing any more. A main view that showed the page tells the engine
/// nobody is looking; a page in its own window stays painted.
fn release(service: &Arc<BrowserService>, label: &str) {
    let Some(binding) = BINDINGS.with(|bindings| bindings.borrow_mut().remove(label)) else {
        return;
    };
    // Only the main view says whether the person is looking; a tab's window never does.
    if detached_window::is_main_window(label) && !service.is_detached(binding.page) {
        service.set_visible(false);
    }
}

fn view_data<'a>(hwnd: HWND) -> Option<&'a mut ViewData> {
    // SAFETY: the pointer was made by `ensure_installed` from a `Box<ViewData>` and is cleared
    // in WM_NCDESTROY; messages come one at a time on the thread that made the window.
    unsafe {
        let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut ViewData;
        pointer.as_mut()
    }
}

fn set_scale(hwnd: HWND, scale: f64) {
    if let Some(data) = view_data(hwnd) {
        data.scale = scale;
    }
}

fn ensure_installed(
    window: &tauri::WebviewWindow,
    service: &Arc<BrowserService>,
    label: &str,
) -> Result<HWND, String> {
    if let Some(existing) = VIEWS.with(|views| views.borrow().get(label).copied()) {
        return Ok(existing);
    }
    let parent = window.hwnd().map_err(|error| error.to_string())?;
    register_class()?;

    let data = Box::new(ViewData {
        service: Arc::clone(service),
        label: label.to_owned(),
        clicks: ClickCounter::default(),
        scale: 1.0,
        tracking_mouse: false,
        high_surrogate: None,
        last_position: (0, 0),
        timer_armed: false,
    });
    let data = Box::into_raw(data);
    // SAFETY: `data` outlives the window: it is freed in WM_NCDESTROY, or right here if the window
    // could not be made.
    let created = unsafe {
        let module = GetModuleHandleW(None).map_err(|error| error.to_string())?;
        CreateWindowExW(
            Default::default(),
            CLASS_NAME,
            w!(""),
            WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS,
            0,
            0,
            1,
            1,
            Some(parent),
            None,
            Some(HINSTANCE(module.0)),
            Some(data as *const c_void),
        )
    };
    match created {
        Ok(hwnd) => {
            VIEWS.with(|views| views.borrow_mut().insert(label.to_owned(), hwnd));
            Ok(hwnd)
        }
        Err(error) => {
            // SAFETY: the window was not made, so nothing else holds the pointer.
            drop(unsafe { Box::from_raw(data) });
            Err(error.to_string())
        }
    }
}

fn register_class() -> Result<(), String> {
    if CLASS_REGISTERED.with(Cell::get) {
        return Ok(());
    }
    // SAFETY: plain Win32 calls with a class that lives as long as the process.
    unsafe {
        let module = GetModuleHandleW(None).map_err(|error| error.to_string())?;
        let class = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(window_procedure),
            hInstance: HINSTANCE(module.0),
            hCursor: LoadCursorW(None, IDC_ARROW).map_err(|error| error.to_string())?,
            lpszClassName: CLASS_NAME,
            ..Default::default()
        };
        if RegisterClassExW(&class) == 0 {
            return Err("could not register the browser view's window class".to_owned());
        }
    }
    CLASS_REGISTERED.with(|registered| registered.set(true));
    Ok(())
}

/// The low and high 16 bits of a message parameter, as signed numbers.
fn low(value: isize) -> i32 {
    i32::from(value as i16)
}

fn high(value: isize) -> i32 {
    i32::from((value >> 16) as i16)
}

fn key_down(virtual_key: i32) -> bool {
    // SAFETY: reads the keyboard state of the calling thread.
    unsafe { GetKeyState(virtual_key) < 0 }
}

/// The modifier bits the protocol uses, from the keyboard's current state and the mouse buttons
/// a mouse message reports.
fn current_modifiers(buttons: usize) -> u32 {
    const MK_LBUTTON: usize = 0x1;
    const MK_RBUTTON: usize = 0x2;
    const MK_MBUTTON: usize = 0x10;
    let mut bits = 0;
    for (down, bit) in [
        (key_down(i32::from(VK_SHIFT.0)), modifiers::SHIFT),
        (key_down(i32::from(VK_CONTROL.0)), modifiers::CONTROL),
        (key_down(i32::from(VK_MENU.0)), modifiers::ALT),
        (
            key_down(i32::from(VK_LWIN.0)) || key_down(i32::from(VK_RWIN.0)),
            modifiers::COMMAND,
        ),
        (buttons & MK_LBUTTON != 0, modifiers::LEFT_BUTTON),
        (buttons & MK_MBUTTON != 0, modifiers::MIDDLE_BUTTON),
        (buttons & MK_RBUTTON != 0, modifiers::RIGHT_BUTTON),
    ] {
        if down {
            bits |= bit;
        }
    }
    bits
}

/// A point of a mouse message, in the page's logical pixels.
fn logical(data: &ViewData, x: i32, y: i32) -> (i32, i32) {
    (
        (f64::from(x) / data.scale).floor() as i32,
        (f64::from(y) / data.scale).floor() as i32,
    )
}

unsafe extern "system" fn window_procedure(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        // SAFETY: for this message `lparam` points to the CREATESTRUCTW of the window being made.
        unsafe {
            let create = &*(lparam.0 as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
        }
        // SAFETY: the default procedure accepts any message.
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    }
    if message == WM_NCDESTROY {
        // SAFETY: the pointer came from `Box::into_raw` and nothing uses it after this message.
        unsafe {
            let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut ViewData;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            if !pointer.is_null() {
                drop(Box::from_raw(pointer));
            }
            return DefWindowProcW(hwnd, message, wparam, lparam);
        }
    }
    let Some(data) = view_data(hwnd) else {
        // SAFETY: the default procedure accepts any message.
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    };
    match handle_message(hwnd, data, message, wparam, lparam) {
        Some(result) => result,
        // SAFETY: the default procedure accepts any message.
        None => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

/// Handles one message, or None to leave it to Windows.
fn handle_message(
    hwnd: HWND,
    data: &mut ViewData,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> Option<LRESULT> {
    let send = |command: PageCommand| {
        // Input goes to the page this window shows. The engine may not be running (or not yet);
        // input then goes nowhere, which is right.
        if let Some(page) = bound_page(&data.label) {
            let _ = data.service.send(page, command);
        }
    };
    match message {
        WM_ERASEBKGND => Some(LRESULT(1)),
        WM_GETDLGCODE => Some(LRESULT(DLGC_WANTALLKEYS as isize)),
        WM_SETCURSOR if low(lparam.0) == HTCLIENT as i32 => {
            // SAFETY: loads and sets a stock cursor.
            unsafe {
                if let Ok(cursor) = LoadCursorW(None, IDC_ARROW) {
                    SetCursor(Some(cursor));
                }
            }
            Some(LRESULT(1))
        }
        WM_PAINT => {
            paint(hwnd, &data.label, data.scale);
            Some(LRESULT(0))
        }
        WM_TIMER if wparam.0 == ANIMATION_TIMER => {
            invalidate(hwnd);
            if !control_is_active() {
                // SAFETY: the handle is this window's.
                unsafe {
                    let _ = KillTimer(Some(hwnd), ANIMATION_TIMER);
                }
                data.timer_armed = false;
            }
            Some(LRESULT(0))
        }
        WM_SETFOCUS => {
            send(PageCommand::Focus { focused: true });
            Some(LRESULT(0))
        }
        WM_KILLFOCUS => {
            send(PageCommand::Focus { focused: false });
            Some(LRESULT(0))
        }
        WM_MOUSEMOVE => {
            let (x, y) = logical(data, low(lparam.0), high(lparam.0));
            data.last_position = (x, y);
            if !data.tracking_mouse {
                data.tracking_mouse = true;
                let mut track = TRACKMOUSEEVENT {
                    cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                // SAFETY: `track` is a valid structure for this call.
                unsafe {
                    let _ = TrackMouseEvent(&mut track);
                }
            }
            send(PageCommand::MouseMove {
                x,
                y,
                modifiers: current_modifiers(wparam.0),
                leave: false,
            });
            Some(LRESULT(0))
        }
        WM_MOUSELEAVE => {
            data.tracking_mouse = false;
            let (x, y) = data.last_position;
            send(PageCommand::MouseMove {
                x,
                y,
                modifiers: current_modifiers(0),
                leave: true,
            });
            Some(LRESULT(0))
        }
        WM_LBUTTONDOWN | WM_MBUTTONDOWN | WM_RBUTTONDOWN => {
            let (button, code) = match message {
                WM_LBUTTONDOWN => (MouseButton::Left, 1),
                WM_MBUTTONDOWN => (MouseButton::Middle, 2),
                _ => (MouseButton::Right, 3),
            };
            // SAFETY: the handle is this window's; focus and capture follow the click.
            unsafe {
                let _ = SetFocus(Some(hwnd));
                SetCapture(hwnd);
            }
            let (x, y) = logical(data, low(lparam.0), high(lparam.0));
            // SAFETY: reads the time of the message being handled.
            let at = Duration::from_millis(u64::from(unsafe { GetMessageTime() } as u32));
            let clicks = data.clicks.press(at, x, y, code);
            send(PageCommand::MouseClick {
                x,
                y,
                button,
                pressed: true,
                clicks,
                modifiers: current_modifiers(wparam.0),
            });
            Some(LRESULT(0))
        }
        WM_LBUTTONUP | WM_MBUTTONUP | WM_RBUTTONUP => {
            let button = match message {
                WM_LBUTTONUP => MouseButton::Left,
                WM_MBUTTONUP => MouseButton::Middle,
                _ => MouseButton::Right,
            };
            // SAFETY: gives the mouse back to everyone.
            unsafe {
                let _ = ReleaseCapture();
            }
            let (x, y) = logical(data, low(lparam.0), high(lparam.0));
            send(PageCommand::MouseClick {
                x,
                y,
                button,
                pressed: false,
                clicks: 1,
                modifiers: current_modifiers(wparam.0),
            });
            Some(LRESULT(0))
        }
        WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
            // The position of a wheel message is on the screen, not in the window.
            let mut point = POINT {
                x: low(lparam.0),
                y: high(lparam.0),
            };
            // SAFETY: converts a point for this window.
            unsafe {
                let _ = ScreenToClient(hwnd, &mut point);
            }
            let (x, y) = logical(data, point.x, point.y);
            let notches = high(wparam.0 as isize);
            let amount = notches * WHEEL_STEP / WHEEL_NOTCH;
            // Up and right are positive for Windows; the engine counts up and left as positive.
            let (delta_x, delta_y) = if message == WM_MOUSEWHEEL {
                (0, amount)
            } else {
                (-amount, 0)
            };
            send(PageCommand::MouseWheel {
                x,
                y,
                delta_x,
                delta_y,
                modifiers: current_modifiers(wparam.0 & 0xffff),
            });
            Some(LRESULT(0))
        }
        WM_KEYDOWN => {
            let virtual_key = wparam.0 as i32;
            let bits = current_modifiers(0);
            // Copy, paste and the like: an off-screen page has no system bindings behind it.
            if let Some(command) = EditCommand::from_shortcut(bits, virtual_key) {
                send(PageCommand::Edit { command });
                return Some(LRESULT(0));
            }
            send(PageCommand::Key {
                kind: KeyKind::Down,
                windows_code: virtual_key,
                native_code: lparam.0 as i32,
                modifiers: bits,
                // Text arrives as WM_CHAR, so a key press carries no character of its own.
                character: 0,
            });
            Some(LRESULT(0))
        }
        WM_KEYUP => {
            send(PageCommand::Key {
                kind: KeyKind::Up,
                windows_code: wparam.0 as i32,
                native_code: lparam.0 as i32,
                modifiers: current_modifiers(0),
                character: 0,
            });
            Some(LRESULT(0))
        }
        WM_CHAR => {
            let unit = wparam.0 as u16;
            // Control characters from shortcuts are not text; Enter, Tab and Backspace are
            // keys the page types as characters.
            if unit < 0x20 && !matches!(unit, 0x08 | 0x09 | 0x0d) {
                return Some(LRESULT(0));
            }
            let text = match (data.high_surrogate.take(), unit) {
                (None, 0xd800..=0xdbff) => {
                    data.high_surrogate = Some(unit);
                    return Some(LRESULT(0));
                }
                (Some(high), 0xdc00..=0xdfff) => String::from_utf16_lossy(&[high, unit]),
                _ => String::from_utf16_lossy(&[unit]),
            };
            send(PageCommand::Text { text });
            Some(LRESULT(0))
        }
        WM_IME_STARTCOMPOSITION => {
            // Windows draws the composition where the form says; the last click is the best
            // guess for where the text goes, since the page does not tell where its caret is.
            // SAFETY: the context is released right after use.
            unsafe {
                let context = ImmGetContext(hwnd);
                if !context.is_invalid() {
                    let form = COMPOSITIONFORM {
                        dwStyle: CFS_POINT,
                        ptCurrentPos: POINT {
                            x: (f64::from(data.last_position.0) * data.scale) as i32,
                            y: (f64::from(data.last_position.1) * data.scale) as i32,
                        },
                        rcArea: RECT::default(),
                    };
                    let _ = ImmSetCompositionWindow(context, &form);
                    let _ = ImmReleaseContext(hwnd, context);
                }
            }
            // The default procedure goes on to show the composition.
            None
        }
        _ => None,
    }
}

/// Draws the page this window shows, from its latest frame, with the agent's marks over it while
/// one is in control.
fn paint(hwnd: HWND, label: &str, scale: f64) {
    let mut paint = PAINTSTRUCT::default();
    // SAFETY: the standard paint sequence for a window of this thread.
    unsafe {
        let device = BeginPaint(hwnd, &mut paint);
        let mut client = RECT::default();
        let _ = GetClientRect(hwnd, &mut client);

        let binding = BINDINGS.with(|bindings| bindings.borrow().get(label).cloned());
        let drawn = FRAMES.with(|frames| {
            let frames = frames.borrow();
            let binding = binding.as_ref()?;
            let frame = frames.get(&binding.page)?;
            let pixels = with_overlay(frame, scale, binding.pointer.as_deref());
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: frame.width as i32,
                    // Negative: the rows are top to bottom, as the engine writes them.
                    biHeight: -(frame.height as i32),
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            SetStretchBltMode(device, HALFTONE);
            StretchDIBits(
                device,
                0,
                0,
                client.right - client.left,
                client.bottom - client.top,
                0,
                0,
                frame.width as i32,
                frame.height as i32,
                Some(pixels.as_ptr().cast()),
                &info,
                DIB_RGB_COLORS,
                SRCCOPY,
            );
            Some(())
        });
        if drawn.is_none() {
            // Before the first frame, or while the engine starts: a plain dark page.
            let brush = CreateSolidBrush(COLORREF(0x001a_1a1a));
            FillRect(device, &client, brush);
            let _ = DeleteObject(brush.into());
        }
        let _ = EndPaint(hwnd, &paint);
    }
}

/// The frame's pixels, with the agent's marks laid over them while the page's agent is in control.
fn with_overlay<'a>(
    frame: &'a Frame,
    scale: f64,
    pointer: Option<&AgentPointer>,
) -> std::borrow::Cow<'a, [u8]> {
    let view = pointer.and_then(|pointer| pointer.view(Instant::now()));
    let Some(view) = view else {
        return std::borrow::Cow::Borrowed(&frame.pixels);
    };
    let size = (
        f64::from(frame.width) / scale,
        f64::from(frame.height) / scale,
    );
    let Some(layer) = overlay_renderer::render(&view, size, scale, clock(), font()) else {
        return std::borrow::Cow::Borrowed(&frame.pixels);
    };
    if layer.width() != frame.width || layer.height() != frame.height {
        return std::borrow::Cow::Borrowed(&frame.pixels);
    }
    let mut pixels = frame.pixels.clone();
    overlay_renderer::composite_over_bgra(&mut pixels, &layer);
    std::borrow::Cow::Owned(pixels)
}
