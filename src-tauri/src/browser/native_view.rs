//! The native view of the page: a GTK drawing area laid over Lynx's webview, in the same window.
//! Frames from the engine are painted into it, and the pointer and keyboard on it go to the
//! engine. The webview itself is moved into a `gtk::Overlay` the first time the view is needed,
//! so an app that never opens the browser keeps its plain layout.
//!
//! GTK objects belong to the main thread; everything that touches them runs there.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gtk::cairo::{Context, FontSlant, FontWeight, Format, ImageSurface};
use gtk::gdk;
use gtk::glib::Propagation;
use gtk::glib::translate::IntoGlib;
use gtk::prelude::*;
use lynx_browser_protocol::message::{EditCommand, KeyKind, MouseButton, ToHost, modifiers};
use tauri::{AppHandle, Manager};

use super::agent_icon;
use super::agent_pointer::{AgentPointer, CursorShape, PointerView};
use super::browser_service::{BrowserService, Frame, FrameSink};
use super::click_counter::ClickCounter;
use super::key_codes::{control_character, windows_key_code};

/// Pixels one notch of the wheel scrolls.
const WHEEL_STEP: f64 = 100.0;

thread_local! {
    // One drawing area per window that has shown the page. Only the active one is visible.
    static AREAS: RefCell<HashMap<String, gtk::DrawingArea>> = RefCell::default();
    // The label of the window the page is shown in right now.
    static ACTIVE: RefCell<Option<String>> = const { RefCell::new(None) };
    // The newest frame, as a surface that owns its pixels, so showing it copies nothing more.
    static FRAME: RefCell<Option<ImageSurface>> = const { RefCell::new(None) };
    // The agent pointer and mark, painted over the frame.
    static POINTER: RefCell<Option<Arc<AgentPointer>>> = const { RefCell::new(None) };
    // A timer is repainting for the pointer's motion.
    static ANIMATING: Cell<bool> = const { Cell::new(false) };
}

/// The mark of an agent in control: the `--agent` token of the theme, converted by hand because
/// the canvas cannot read CSS.
const AGENT_RGB: (f64, f64, f64) = (0.958, 0.592, 0.323);
/// The cursor is drawn this much larger than a system pointer, so it is easy to follow.
const CURSOR_SCALE: f64 = 1.3;
const FRAME_INTERVAL: Duration = Duration::from_millis(16);

struct Shared {
    latest: Mutex<Option<Frame>>,
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
                latest: Mutex::new(None),
                queued: AtomicBool::new(false),
            }),
        }
    }
}

impl FrameSink for NativeView {
    fn frame(&self, frame: Frame) {
        *self
            .shared
            .latest
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(frame);
        if self.shared.queued.swap(true, Ordering::AcqRel) {
            return;
        }
        let shared = Arc::clone(&self.shared);
        let _ = self.app.run_on_main_thread(move || {
            shared.queued.store(false, Ordering::Release);
            let frame = shared
                .latest
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .take();
            if let Some(frame) = frame {
                show_frame(frame);
            }
        });
    }

    fn pointer_changed(&self) {
        let _ = self.app.run_on_main_thread(animate_pointer);
    }
}

/// Repaints every frame for as long as an agent is in control, so the pointer glides and the
/// marks fade; when control ends, one last repaint clears them.
fn animate_pointer() {
    redraw_active();
    if ANIMATING.with(|animating| animating.replace(true)) {
        return;
    }
    gtk::glib::timeout_add_local(FRAME_INTERVAL, || {
        redraw_active();
        let alive = POINTER.with(|pointer| {
            pointer
                .borrow()
                .as_ref()
                .is_some_and(|pointer| pointer.view(Instant::now()).is_some())
        });
        if alive {
            gtk::glib::ControlFlow::Continue
        } else {
            ANIMATING.with(|animating| animating.set(false));
            gtk::glib::ControlFlow::Break
        }
    });
}

fn redraw_active() {
    let Some(label) = ACTIVE.with(|active| active.borrow().clone()) else {
        return;
    };
    AREAS.with(|areas| {
        if let Some(area) = areas.borrow().get(&label) {
            area.queue_draw();
        }
    });
}

/// Puts the view over the page at `rect` in the window `label` (logical pixels, from the
/// webview's top-left corner), installing it first if needed, and tells the engine the size to
/// render at. The page is shown in one window at a time: any other loses it. Main thread only.
pub fn place(
    app: &AppHandle,
    service: &Arc<BrowserService>,
    label: &str,
    rect: (f64, f64, f64, f64),
) -> Result<(), String> {
    ensure_installed(app, service, label)?;
    let (x, y, width, height) = rect;
    AREAS.with(|areas| {
        let areas = areas.borrow();
        for (other, area) in areas.iter() {
            if other != label {
                area.set_visible(false);
            }
        }
        let area = areas
            .get(label)
            .ok_or("the browser view is not installed")?;
        let width = width.round().max(1.0) as i32;
        let height = height.round().max(1.0) as i32;
        area.set_margin_start(x.round() as i32);
        area.set_margin_top(y.round() as i32);
        area.set_size_request(width, height);
        area.set_visible(true);
        area.queue_draw();
        service.resize(width as u32, height as u32, area.scale_factor() as f32);
        service.set_visible(true);
        Ok::<(), String>(())
    })?;
    ACTIVE.with(|active| *active.borrow_mut() = Some(label.to_owned()));
    Ok(())
}

/// Takes the view off the window `label`. When that window was showing the page, the engine is
/// told nobody is looking. A window that is not the active one changes nothing about the engine:
/// its hide can arrive after the page already moved to another window. Main thread only.
pub fn hide(service: &Arc<BrowserService>, label: &str) {
    AREAS.with(|areas| {
        if let Some(area) = areas.borrow().get(label) {
            area.set_visible(false);
        }
    });
    let was_active = ACTIVE.with(|active| {
        let mut active = active.borrow_mut();
        let was = active.as_deref() == Some(label);
        if was {
            *active = None;
        }
        was
    });
    if was_active {
        service.set_visible(false);
    }
}

/// The window `label` is gone, and its view with it. Main thread only.
pub fn forget(service: &Arc<BrowserService>, label: &str) {
    AREAS.with(|areas| areas.borrow_mut().remove(label));
    let was_active = ACTIVE.with(|active| {
        let mut active = active.borrow_mut();
        let was = active.as_deref() == Some(label);
        if was {
            *active = None;
        }
        was
    });
    if was_active {
        service.set_visible(false);
    }
}

fn show_frame(frame: Frame) {
    let format = Format::ARgb32;
    let surface = format.stride_for_width(frame.width).and_then(|stride| {
        ImageSurface::create_for_data(
            frame.pixels,
            format,
            frame.width as i32,
            frame.height as i32,
            stride,
        )
    });
    let Ok(surface) = surface else {
        return;
    };
    let Some(label) = ACTIVE.with(|active| active.borrow().clone()) else {
        return;
    };
    AREAS.with(|areas| {
        if let Some(area) = areas.borrow().get(&label) {
            // The frame is in device pixels; the area is in logical ones.
            let scale = f64::from(area.scale_factor());
            surface.set_device_scale(scale, scale);
            FRAME.with(|frame| *frame.borrow_mut() = Some(surface));
            area.queue_draw();
        }
    });
}

fn ensure_installed(
    app: &AppHandle,
    service: &Arc<BrowserService>,
    label: &str,
) -> Result<(), String> {
    if AREAS.with(|areas| areas.borrow().contains_key(label)) {
        return Ok(());
    }
    let window = app
        .get_webview_window(label)
        .ok_or_else(|| format!("there is no window {label}"))?;
    let vbox = window.default_vbox().map_err(|error| error.to_string())?;
    let webview = vbox
        .children()
        .into_iter()
        .next()
        .ok_or("the window has no webview")?;
    vbox.remove(&webview);

    let overlay = gtk::Overlay::new();
    overlay.add(&webview);
    let area = gtk::DrawingArea::new();
    area.set_halign(gtk::Align::Start);
    area.set_valign(gtk::Align::Start);
    // Hidden until the first placement.
    area.set_no_show_all(true);
    POINTER.with(|pointer| *pointer.borrow_mut() = Some(service.pointer()));
    connect_drawing(&area);
    connect_input(&area, service);
    overlay.add_overlay(&area);

    vbox.pack_start(&overlay, true, true, 0);
    vbox.show_all();
    AREAS.with(|areas| areas.borrow_mut().insert(label.to_owned(), area));
    Ok(())
}

fn connect_drawing(area: &gtk::DrawingArea) {
    area.connect_draw(|area, context| {
        // Before the first frame, or while the engine starts: a plain dark page.
        context.set_source_rgb(0.1, 0.1, 0.1);
        let _ = context.paint();
        FRAME.with(|frame| {
            if let Some(surface) = frame.borrow().as_ref() {
                let _ = context.set_source_surface(surface, 0.0, 0.0);
                let _ = context.paint();
            }
        });
        let view = POINTER.with(|pointer| {
            pointer
                .borrow()
                .as_ref()
                .and_then(|pointer| pointer.view(Instant::now()))
        });
        if let Some(view) = view {
            let size = (
                f64::from(area.allocated_width()),
                f64::from(area.allocated_height()),
            );
            draw_agent(context, size, &view, clock());
        }
        Propagation::Stop
    });
}

/// Seconds since the first call, for the ring that turns on its own.
fn clock() -> f64 {
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs_f64()
}

/// The longest label drawn, in characters; the rest is cut with an ellipsis.
const MAX_LABEL_CHARS: usize = 38;

/// The frame around the page, the pointer with the agent's mark at its corner, and beside it what
/// the agent is doing: a label, or a spinning ring while a page loads.
fn draw_agent(context: &Context, (width, height): (f64, f64), view: &PointerView, seconds: f64) {
    let (red, green, blue) = AGENT_RGB;
    let strength = view.strength;

    context.set_source_rgba(red, green, blue, 0.9 * strength);
    context.set_line_width(3.0);
    context.rectangle(1.5, 1.5, width - 3.0, height - 3.0);
    let _ = context.stroke();

    let Some((x, y)) = view.at else {
        return;
    };
    if let Some(progress) = view.ripple {
        context.set_source_rgba(red, green, blue, 0.55 * (1.0 - progress) * strength);
        context.set_line_width(2.5);
        context.arc(x, y, 6.0 + 22.0 * progress, 0.0, std::f64::consts::TAU);
        let _ = context.stroke();
    }

    let (corner, middle) = draw_cursor(context, (x, y), view.cursor, strength);

    // The agent's mark sits on a dark disc at the cursor's lower right corner.
    let mut next_to = corner.0 + 6.0;
    let mut anchor_y = middle;
    if agent_icon::has_mark(&view.harness) {
        context.set_source_rgba(0.09, 0.09, 0.11, 0.95 * strength);
        context.arc(corner.0, corner.1, BADGE_RADIUS, 0.0, std::f64::consts::TAU);
        let _ = context.fill_preserve();
        context.set_source_rgba(1.0, 1.0, 1.0, 0.45 * strength);
        context.set_line_width(1.0);
        let _ = context.stroke();
        agent_icon::draw(
            context,
            &view.harness,
            corner,
            2.0 * BADGE_RADIUS - 6.0,
            strength,
        );
        next_to = corner.0 + BADGE_RADIUS + 6.0;
        anchor_y = corner.1;
    }

    // What the agent does goes beside the mark, and flips inside the page near its right edge.
    let top = (anchor_y - LABEL_HEIGHT / 2.0).clamp(4.0, (height - LABEL_HEIGHT - 4.0).max(4.0));
    if view.loading {
        let radius = LABEL_HEIGHT / 2.0;
        let left = if next_to + 2.0 * radius > width {
            x - 8.0 - 2.0 * radius
        } else {
            next_to
        };
        draw_spinner(
            context,
            (left + radius, top + radius),
            radius,
            seconds,
            strength,
        );
        return;
    }
    // Clicking, typing, scrolling and choosing show themselves in the cursor: no label.
    if !view.label.is_empty() {
        draw_label(context, (width, next_to, x, top), view);
    }
}

/// The arrow (tip at the point) or the text cursor (centred on it). Returns where the agent's mark
/// goes, at the cursor's lower right, and the height of the cursor's middle.
fn draw_cursor(
    context: &Context,
    (x, y): (f64, f64),
    shape: CursorShape,
    strength: f64,
) -> ((f64, f64), f64) {
    let scale = CURSOR_SCALE;
    match shape {
        CursorShape::Arrow => {
            context.new_path();
            for (index, (dx, dy)) in [
                (0.0, 0.0),
                (0.0, 17.0),
                (4.5, 13.0),
                (7.5, 20.0),
                (10.0, 19.0),
                (7.0, 12.0),
                (12.5, 12.0),
            ]
            .into_iter()
            .enumerate()
            {
                if index == 0 {
                    context.move_to(x + dx * scale, y + dy * scale);
                } else {
                    context.line_to(x + dx * scale, y + dy * scale);
                }
            }
            context.close_path();
            context.set_source_rgba(1.0, 1.0, 1.0, strength);
            let _ = context.fill_preserve();
            context.set_source_rgba(0.05, 0.05, 0.08, strength);
            context.set_line_width(1.8);
            let _ = context.stroke();
            (
                (x + 12.5 * scale + 1.5, y + 20.0 * scale - 2.0),
                y + 14.0 * scale,
            )
        }
        CursorShape::IBeam => {
            let (half_height, half_width) = (10.0 * scale, 4.0 * scale);
            let draw_beam = |context: &Context| {
                context.new_path();
                context.move_to(x - half_width, y - half_height);
                context.line_to(x + half_width, y - half_height);
                context.move_to(x, y - half_height);
                context.line_to(x, y + half_height);
                context.move_to(x - half_width, y + half_height);
                context.line_to(x + half_width, y + half_height);
            };
            // A light outline under a dark line, so it shows on any page.
            context.set_line_cap(gtk::cairo::LineCap::Round);
            draw_beam(context);
            context.set_source_rgba(1.0, 1.0, 1.0, strength);
            context.set_line_width(4.2);
            let _ = context.stroke();
            draw_beam(context);
            context.set_source_rgba(0.05, 0.05, 0.08, strength);
            context.set_line_width(1.8);
            let _ = context.stroke();
            ((x + half_width + 9.0, y + half_height), y)
        }
    }
}

const BADGE_RADIUS: f64 = 9.0;
const LABEL_HEIGHT: f64 = 22.0;

fn draw_spinner(context: &Context, centre: (f64, f64), radius: f64, seconds: f64, strength: f64) {
    let (red, green, blue) = AGENT_RGB;
    context.set_source_rgba(red, green, blue, strength);
    context.arc(centre.0, centre.1, radius, 0.0, std::f64::consts::TAU);
    let _ = context.fill();
    // A three-quarter ring turning about once a second.
    let turn = seconds * std::f64::consts::TAU * 1.1;
    context.set_source_rgba(0.1, 0.07, 0.04, strength);
    context.set_line_width(2.4);
    context.set_line_cap(gtk::cairo::LineCap::Round);
    context.arc(
        centre.0,
        centre.1,
        radius - 6.0,
        turn,
        turn + std::f64::consts::PI * 1.5,
    );
    let _ = context.stroke();
}

/// (page width, where the label starts, the arrow's x, the label's top)
type LabelSpot = (f64, f64, f64, f64);

fn draw_label(context: &Context, (width, start, arrow_x, top): LabelSpot, view: &PointerView) {
    let (red, green, blue) = AGENT_RGB;
    let strength = view.strength;
    let text = clipped(&view.label);
    context.select_font_face("sans-serif", FontSlant::Normal, FontWeight::Bold);
    context.set_font_size(12.0);
    let Ok(extents) = context.text_extents(&text) else {
        return;
    };
    let label_width = extents.width() + 16.0;
    let left = if start + label_width > width {
        arrow_x - 8.0 - label_width
    } else {
        start
    };

    rounded_rectangle(context, left, top, label_width, LABEL_HEIGHT, 6.0);
    context.set_source_rgba(red, green, blue, strength);
    let _ = context.fill();
    context.set_source_rgba(0.1, 0.07, 0.04, strength);
    context.move_to(left + 8.0, top + 15.0);
    let _ = context.show_text(&text);
}

fn clipped(label: &str) -> String {
    if label.chars().count() <= MAX_LABEL_CHARS {
        return label.to_owned();
    }
    let cut: String = label.chars().take(MAX_LABEL_CHARS - 1).collect();
    format!("{cut}…")
}

fn rounded_rectangle(context: &Context, x: f64, y: f64, width: f64, height: f64, radius: f64) {
    use std::f64::consts::{FRAC_PI_2, PI};
    context.new_sub_path();
    context.arc(x + width - radius, y + radius, radius, -FRAC_PI_2, 0.0);
    context.arc(
        x + width - radius,
        y + height - radius,
        radius,
        0.0,
        FRAC_PI_2,
    );
    context.arc(x + radius, y + height - radius, radius, FRAC_PI_2, PI);
    context.arc(x + radius, y + radius, radius, PI, 3.0 * FRAC_PI_2);
    context.close_path();
}

fn modifiers_of(state: gdk::ModifierType) -> u32 {
    let mut bits = 0;
    for (flag, bit) in [
        (gdk::ModifierType::SHIFT_MASK, modifiers::SHIFT),
        (gdk::ModifierType::CONTROL_MASK, modifiers::CONTROL),
        (gdk::ModifierType::MOD1_MASK, modifiers::ALT),
        (gdk::ModifierType::BUTTON1_MASK, modifiers::LEFT_BUTTON),
        (gdk::ModifierType::BUTTON2_MASK, modifiers::MIDDLE_BUTTON),
        (gdk::ModifierType::BUTTON3_MASK, modifiers::RIGHT_BUTTON),
    ] {
        if state.contains(flag) {
            bits |= bit;
        }
    }
    bits
}

fn connect_input(area: &gtk::DrawingArea, service: &Arc<BrowserService>) {
    area.set_can_focus(true);
    area.add_events(
        gdk::EventMask::POINTER_MOTION_MASK
            | gdk::EventMask::BUTTON_PRESS_MASK
            | gdk::EventMask::BUTTON_RELEASE_MASK
            | gdk::EventMask::SCROLL_MASK
            | gdk::EventMask::SMOOTH_SCROLL_MASK
            | gdk::EventMask::KEY_PRESS_MASK
            | gdk::EventMask::KEY_RELEASE_MASK
            | gdk::EventMask::LEAVE_NOTIFY_MASK
            | gdk::EventMask::FOCUS_CHANGE_MASK,
    );

    // The engine may not be running (or not yet); input then goes nowhere, which is right.
    let send = {
        let service = Arc::clone(service);
        move |command: ToHost| {
            let _ = service.send(command);
        }
    };

    area.connect_motion_notify_event({
        let send = send.clone();
        move |_, event| {
            let (x, y) = event.position();
            send(ToHost::MouseMove {
                x: x as i32,
                y: y as i32,
                modifiers: modifiers_of(event.state()),
                leave: false,
            });
            Propagation::Stop
        }
    });
    area.connect_leave_notify_event({
        let send = send.clone();
        move |_, event| {
            let (x, y) = event.position();
            send(ToHost::MouseMove {
                x: x as i32,
                y: y as i32,
                modifiers: modifiers_of(event.state()),
                leave: true,
            });
            Propagation::Proceed
        }
    });

    let clicks = Rc::new(RefCell::new(ClickCounter::default()));
    area.connect_button_press_event({
        let send = send.clone();
        let clicks = Rc::clone(&clicks);
        move |area, event| {
            // GTK adds its own double and triple click events; the count is kept by ClickCounter.
            if event.event_type() != gdk::EventType::ButtonPress {
                return Propagation::Stop;
            }
            area.grab_focus();
            let Some(button) = mouse_button(event.button()) else {
                return Propagation::Stop;
            };
            let (x, y) = event.position();
            let (x, y) = (x as i32, y as i32);
            let count = clicks.borrow_mut().press(
                Duration::from_millis(u64::from(event.time())),
                x,
                y,
                event.button(),
            );
            send(ToHost::MouseClick {
                x,
                y,
                button,
                pressed: true,
                clicks: count,
                modifiers: modifiers_of(event.state()),
            });
            Propagation::Stop
        }
    });
    area.connect_button_release_event({
        let send = send.clone();
        move |_, event| {
            if let Some(button) = mouse_button(event.button()) {
                let (x, y) = event.position();
                send(ToHost::MouseClick {
                    x: x as i32,
                    y: y as i32,
                    button,
                    pressed: false,
                    clicks: 1,
                    modifiers: modifiers_of(event.state()),
                });
            }
            Propagation::Stop
        }
    });
    area.connect_scroll_event({
        let send = send.clone();
        move |_, event| {
            let (x, y) = event.position();
            // GTK counts down as positive; the engine counts up as positive.
            let (dx, dy) = match event.direction() {
                gdk::ScrollDirection::Up => (0.0, 1.0),
                gdk::ScrollDirection::Down => (0.0, -1.0),
                gdk::ScrollDirection::Left => (1.0, 0.0),
                gdk::ScrollDirection::Right => (-1.0, 0.0),
                gdk::ScrollDirection::Smooth => {
                    let (dx, dy) = event.delta();
                    (-dx, -dy)
                }
                _ => (0.0, 0.0),
            };
            if dx != 0.0 || dy != 0.0 {
                send(ToHost::MouseWheel {
                    x: x as i32,
                    y: y as i32,
                    delta_x: (dx * WHEEL_STEP) as i32,
                    delta_y: (dy * WHEEL_STEP) as i32,
                    modifiers: modifiers_of(event.state()),
                });
            }
            Propagation::Stop
        }
    });

    connect_keyboard(area, send);
}

fn mouse_button(button: u32) -> Option<MouseButton> {
    match button {
        1 => Some(MouseButton::Left),
        2 => Some(MouseButton::Middle),
        3 => Some(MouseButton::Right),
        _ => None,
    }
}

/// Keys go through the input method first, which composes accents and dead keys into text; what
/// it leaves alone goes to the engine as key events, plus the character for printable keys.
fn connect_keyboard(area: &gtk::DrawingArea, send: impl Fn(ToHost) + Clone + 'static) {
    let input_method = gtk::IMMulticontext::new();
    input_method.connect_commit({
        let send = send.clone();
        move |_, text| {
            send(ToHost::Text {
                text: text.to_owned(),
            })
        }
    });
    area.connect_realize({
        let input_method = input_method.clone();
        move |area| input_method.set_client_window(area.window().as_ref())
    });
    area.connect_focus_in_event({
        let (send, input_method) = (send.clone(), input_method.clone());
        move |_, _| {
            input_method.focus_in();
            send(ToHost::Focus { focused: true });
            Propagation::Proceed
        }
    });
    area.connect_focus_out_event({
        let (send, input_method) = (send.clone(), input_method.clone());
        move |_, _| {
            input_method.focus_out();
            send(ToHost::Focus { focused: false });
            Propagation::Proceed
        }
    });

    // Keys whose press went to the engine, so only their release does too.
    let pressed: Rc<RefCell<HashSet<u16>>> = Rc::default();
    area.connect_key_press_event({
        let (send, input_method, pressed) =
            (send.clone(), input_method.clone(), Rc::clone(&pressed));
        move |_, event| {
            if input_method.filter_keypress(event) {
                return Propagation::Stop;
            }
            let keyval = event.keyval();
            let keysym: u32 = keyval.into_glib();
            let state = event.state();
            let hardware = event.hardware_keycode();
            // Copy, paste and the like: an off-screen page has no system bindings behind it.
            if let Some(command) =
                EditCommand::from_shortcut(modifiers_of(state), windows_key_code(keysym))
            {
                send(ToHost::Edit { command });
                return Propagation::Stop;
            }
            send(ToHost::Key {
                kind: KeyKind::Down,
                windows_code: windows_key_code(keysym),
                native_code: i32::from(hardware),
                modifiers: modifiers_of(state),
                character: control_character(keysym),
            });
            pressed.borrow_mut().insert(hardware);
            // A shortcut (Ctrl+C) is not text.
            let shortcut =
                state.intersects(gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::MOD1_MASK);
            let character = keyval
                .to_unicode()
                .filter(|character| !character.is_control() && !shortcut);
            if let Some(character) = character {
                send(ToHost::Text {
                    text: character.to_string(),
                });
            }
            Propagation::Stop
        }
    });
    area.connect_key_release_event({
        let (send, input_method) = (send, input_method);
        move |_, event| {
            if input_method.filter_keypress(event) {
                return Propagation::Stop;
            }
            let hardware = event.hardware_keycode();
            if pressed.borrow_mut().remove(&hardware) {
                send(ToHost::Key {
                    kind: KeyKind::Up,
                    windows_code: windows_key_code(event.keyval().into_glib()),
                    native_code: i32::from(hardware),
                    modifiers: modifiers_of(event.state()),
                    character: 0,
                });
            }
            Propagation::Stop
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn painted(view: &PointerView) -> ImageSurface {
        let surface = ImageSurface::create(Format::ARgb32, 400, 300).expect("surface");
        let context = Context::new(&surface).expect("context");
        context.set_source_rgb(1.0, 1.0, 1.0);
        context.paint().expect("paint");
        draw_agent(&context, (400.0, 300.0), view, 0.3);
        drop(context);
        surface
    }

    fn pixel(surface: &mut ImageSurface, x: usize, y: usize) -> [u8; 4] {
        let stride = surface.stride() as usize;
        let data = surface.data().expect("pixels");
        let at = y * stride + x * 4;
        [data[at], data[at + 1], data[at + 2], data[at + 3]]
    }

    fn view(label: &str, busy: bool, loading: bool) -> PointerView {
        PointerView {
            harness: "claude".to_owned(),
            label: label.to_owned(),
            cursor: CursorShape::Arrow,
            busy,
            loading,
            at: Some((100.0, 80.0)),
            ripple: None,
            strength: 1.0,
        }
    }

    fn save(surface: &mut ImageSurface, name: &str) {
        if let Some(folder) = std::env::var_os("LYNX_RENDER_PREVIEW") {
            // Raw BGRA, 400x300; a script turns it into a picture.
            let data = surface.data().expect("pixels").to_vec();
            std::fs::write(std::path::Path::new(&folder).join(name), data).expect("write");
        }
    }

    #[test]
    fn the_frame_arrow_mark_and_label_are_painted_over_the_page() {
        let mut surface = painted(&view("Opening example.com", true, false));

        // The frame around the page is in the agent's colour.
        assert_ne!(pixel(&mut surface, 1, 150), [255, 255, 255, 255]);
        // The mark's disc sits at the arrow's lower right corner.
        assert_ne!(pixel(&mut surface, 112, 108), [255, 255, 255, 255]);
        // The label beside it is filled in.
        assert_ne!(pixel(&mut surface, 150, 108), [255, 255, 255, 255]);
        // Far from everything the page stays white.
        assert_eq!(pixel(&mut surface, 300, 250), [255, 255, 255, 255]);
        save(&mut surface, "label.bgra");
    }

    #[test]
    fn while_a_page_loads_a_ring_takes_the_labels_place() {
        let mut surface = painted(&view("Opening example.com", true, true));

        // Where the label's middle would be, only the small ring's disc is.
        assert_ne!(pixel(&mut surface, 140, 108), [255, 255, 255, 255]);
        assert_eq!(pixel(&mut surface, 190, 108), [255, 255, 255, 255]);
        save(&mut surface, "ring.bgra");
    }

    #[test]
    fn with_no_pointer_yet_only_the_frame_is_painted() {
        let mut still_nowhere = view("Reading the page", true, false);
        still_nowhere.at = None;
        let mut surface = painted(&still_nowhere);

        assert_ne!(pixel(&mut surface, 1, 150), [255, 255, 255, 255]);
        assert_eq!(pixel(&mut surface, 150, 108), [255, 255, 255, 255]);
    }

    #[test]
    fn a_tool_that_shows_itself_has_no_label() {
        let mut surface = painted(&view("", false, false));

        // The cursor and its mark are there; the label's place is empty.
        assert_ne!(pixel(&mut surface, 112, 108), [255, 255, 255, 255]);
        assert_eq!(pixel(&mut surface, 150, 105), [255, 255, 255, 255]);
    }

    #[test]
    fn typing_turns_the_arrow_into_the_text_cursor() {
        let mut typing = view("", false, false);
        typing.cursor = CursorShape::IBeam;
        let mut surface = painted(&typing);

        // The stem runs through the point, and the arrow's body is not there.
        assert_ne!(pixel(&mut surface, 100, 80), [255, 255, 255, 255]);
        assert_eq!(pixel(&mut surface, 105, 70), [255, 255, 255, 255]);
        save(&mut surface, "ibeam.bgra");
    }

    #[test]
    fn a_long_label_is_cut_with_an_ellipsis() {
        assert_eq!(clipped("short"), "short");
        let cut = clipped(&"x".repeat(80));
        assert_eq!(cut.chars().count(), MAX_LABEL_CHARS);
        assert!(cut.ends_with('…'));
    }
}
