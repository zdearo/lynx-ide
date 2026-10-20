//! The pointer an agent moves on the page, and whether an agent is in control. Both are drawn over
//! the page by the native view, so the person sees where the agent is, what it is doing and who it
//! is. This holds the state and the motion; it draws nothing and knows no clock of its own.

use std::f64::consts::{PI, TAU};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

/// How long control lasts after an agent's last action, when nothing else says it is still there.
pub const CONTROL_HOLD: Duration = Duration::from_secs(6);
/// While the agent's session is working it is likely thinking about its next step, so control
/// stays longer; the session going quiet ends it sooner.
const WORKING_HOLD: Duration = Duration::from_secs(30);
/// What is left of control once the session stops working.
const RELEASE_HOLD: Duration = Duration::from_millis(1500);
/// The last stretch of control, in which the marks fade out.
const FADE: Duration = Duration::from_millis(600);
const MIN_TRAVEL: Duration = Duration::from_millis(220);
const MAX_TRAVEL: Duration = Duration::from_millis(600);
/// Pixels per second the pointer covers on a long trip.
const SPEED: f64 = 2200.0;
const RIPPLE: Duration = Duration::from_millis(450);
/// The marks fade in rather than appear.
const FADE_IN: Duration = Duration::from_millis(250);

/// Who is in control: the terminal session the agent runs in, and which agent it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Controller {
    pub session: String,
    /// The harness name (`claude`, `codex`...), which picks the icon. Empty when unknown.
    pub harness: String,
}

/// What the pointer looks like: an arrow, or the text cursor over a field being typed in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CursorShape {
    #[default]
    Arrow,
    IBeam,
}

/// A drift of the pointer while the agent reads or scrolls, on top of where it rests.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sweep {
    /// Sideways reach in pixels, and how many back-and-forths it makes. Zero for a straight sweep.
    pub sideways: f64,
    pub wiggles: f64,
    /// How far down it goes and back (negative goes up).
    pub down: f64,
    pub duration: Duration,
}

impl Sweep {
    pub fn scrolling(down: bool) -> Self {
        Self {
            sideways: 0.0,
            wiggles: 0.0,
            down: if down { 70.0 } else { -70.0 },
            duration: Duration::from_millis(700),
        }
    }
}

struct Position {
    from: (f64, f64),
    to: (f64, f64),
    started: Instant,
    travel: Duration,
}

struct State {
    /// When control began, for the fade-in.
    since: Instant,
    who: Controller,
    label: String,
    /// A tool is running now.
    busy: bool,
    /// A page is loading because of the agent.
    loading: bool,
    last_action: Instant,
    /// The agent's session is working, as the terminal sees it.
    working: bool,
    position: Option<Position>,
    sweep: Option<(Instant, Sweep)>,
    clicked: Option<Instant>,
    cursor: CursorShape,
}

/// What to paint right now.
#[derive(Debug, Clone, PartialEq)]
pub struct PointerView {
    pub harness: String,
    /// What the agent is doing, in words. Empty when the pointer says it by itself.
    pub label: String,
    pub cursor: CursorShape,
    pub busy: bool,
    /// A page is loading: the label gives way to a spinning ring.
    pub loading: bool,
    /// Where the pointer is, in the page's pixels, or None before it has been anywhere.
    pub at: Option<(f64, f64)>,
    /// 0 to 1, how far the click ripple has grown. None when there is none.
    pub ripple: Option<f64>,
    /// 1 while in control, falling to 0 as control runs out.
    pub strength: f64,
}

#[derive(Default)]
pub struct AgentPointer {
    state: Mutex<Option<State>>,
}

impl AgentPointer {
    fn lock(&self) -> MutexGuard<'_, Option<State>> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// An agent starts a tool: control starts or continues, and the label says what it does.
    pub fn begin(&self, who: &Controller, label: &str, now: Instant) {
        let mut state = self.lock();
        match state.as_mut() {
            Some(current) => {
                // Another agent takes the pointer where it stands.
                current.who = who.clone();
                label.clone_into(&mut current.label);
                current.busy = true;
                current.loading = false;
                current.last_action = now;
            }
            None => {
                *state = Some(State {
                    since: now,
                    who: who.clone(),
                    label: label.to_owned(),
                    busy: true,
                    loading: false,
                    last_action: now,
                    working: false,
                    position: None,
                    sweep: None,
                    clicked: None,
                    cursor: CursorShape::default(),
                });
            }
        }
    }

    /// The tool is done: the label stays, still, until the next action.
    pub fn end(&self, now: Instant) {
        if let Some(state) = self.lock().as_mut() {
            state.busy = false;
            state.loading = false;
            state.last_action = now;
        }
    }

    pub fn set_cursor(&self, cursor: CursorShape) {
        if let Some(state) = self.lock().as_mut() {
            state.cursor = cursor;
        }
    }

    pub fn set_loading(&self, loading: bool) {
        if let Some(state) = self.lock().as_mut() {
            state.loading = loading;
        }
    }

    /// Sends the pointer to `to`. Returns how long the trip takes, so the caller can wait for it
    /// before it clicks.
    pub fn move_to(&self, to: (f64, f64), now: Instant) -> Duration {
        let mut guard = self.lock();
        let Some(state) = guard.as_mut() else {
            return Duration::ZERO;
        };
        // The first trip glides in from off to one side rather than appearing in place.
        // A trip always starts from where the pointer is shown, drift included, so it never jumps.
        let from = displayed(state, now).unwrap_or((to.0 - 140.0, to.1 - 90.0));
        let distance = (to.0 - from.0).hypot(to.1 - from.1);
        let travel = Duration::from_secs_f64(distance / SPEED).clamp(MIN_TRAVEL, MAX_TRAVEL);
        state.position = Some(Position {
            from,
            to,
            started: now,
            travel,
        });
        state.sweep = None;
        travel
    }

    /// Makes the pointer drift while the agent reads or scrolls.
    pub fn sweep(&self, sweep: Sweep, now: Instant) {
        if let Some(state) = self.lock().as_mut() {
            state.sweep = Some((now, sweep));
        }
    }

    /// Whether the pointer has been anywhere yet.
    pub fn is_placed(&self) -> bool {
        self.lock()
            .as_ref()
            .is_some_and(|state| state.position.is_some())
    }

    /// The pointer presses: the ripple starts.
    pub fn click(&self, now: Instant) {
        if let Some(state) = self.lock().as_mut() {
            state.clicked = Some(now);
            state.last_action = now;
        }
    }

    /// The terminal session `session` is working, or stopped. Control follows its agent: it stays
    /// while the agent works, and winds down soon after the agent stops.
    pub fn session_status(&self, session: &str, working: bool, now: Instant) {
        let mut guard = self.lock();
        let Some(state) = guard.as_mut().filter(|state| state.who.session == session) else {
            return;
        };
        state.working = working;
        if !working {
            let latest_end = now + RELEASE_HOLD;
            let current_end = state.last_action + CONTROL_HOLD;
            if current_end > latest_end {
                state.last_action = latest_end.checked_sub(CONTROL_HOLD).unwrap_or(now);
            }
        }
    }

    /// The marks to paint at `now`, or None when no agent is in control.
    pub fn view(&self, now: Instant) -> Option<PointerView> {
        let mut guard = self.lock();
        let state = guard.as_ref()?;
        let hold = if state.working {
            WORKING_HOLD
        } else {
            CONTROL_HOLD
        };
        let idle = now.saturating_duration_since(state.last_action);
        if !state.busy && idle >= hold {
            *guard = None;
            return None;
        }
        let state = guard.as_ref()?;
        let remaining = hold.saturating_sub(idle);
        let fade_out = if state.busy || remaining >= FADE {
            1.0
        } else {
            remaining.as_secs_f64() / FADE.as_secs_f64()
        };
        let fade_in = (now.saturating_duration_since(state.since).as_secs_f64()
            / FADE_IN.as_secs_f64())
        .min(1.0);
        let strength = fade_out.min(fade_in);
        let ripple = state
            .clicked
            .map(|at| now.saturating_duration_since(at))
            .filter(|elapsed| *elapsed < RIPPLE)
            .map(|elapsed| elapsed.as_secs_f64() / RIPPLE.as_secs_f64());
        Some(PointerView {
            harness: state.who.harness.clone(),
            label: state.label.clone(),
            cursor: state.cursor,
            busy: state.busy,
            loading: state.loading,
            at: displayed(state, now),
            ripple,
            strength,
        })
    }

    /// Gives control back at once, as when the page goes away.
    pub fn release(&self) {
        *self.lock() = None;
    }
}

/// Where the pointer is shown: where it rests, plus the drift of a sweep under way.
fn displayed(state: &State, now: Instant) -> Option<(f64, f64)> {
    let (x, y) = position_at(state.position.as_ref()?, now);
    let (dx, dy) = state.sweep.map_or((0.0, 0.0), |(started, sweep)| {
        sweep_offset(&sweep, now.saturating_duration_since(started))
    });
    Some((x + dx, y + dy))
}

fn position_at(position: &Position, now: Instant) -> (f64, f64) {
    let elapsed = now.saturating_duration_since(position.started);
    let progress = (elapsed.as_secs_f64() / position.travel.as_secs_f64()).clamp(0.0, 1.0);
    // Slow at both ends, like a hand.
    let eased = progress * progress * (3.0 - 2.0 * progress);
    (
        position.from.0 + (position.to.0 - position.from.0) * eased,
        position.from.1 + (position.to.1 - position.from.1) * eased,
    )
}

fn sweep_offset(sweep: &Sweep, elapsed: Duration) -> (f64, f64) {
    let progress = elapsed.as_secs_f64() / sweep.duration.as_secs_f64();
    if progress >= 1.0 {
        return (0.0, 0.0);
    }
    (
        sweep.sideways * (TAU * sweep.wiggles * progress).sin(),
        sweep.down * (PI * progress).sin(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn who(session: &str) -> Controller {
        Controller {
            session: session.to_owned(),
            harness: "claude".to_owned(),
        }
    }

    fn at(start: Instant, millis: u64) -> Instant {
        start + Duration::from_millis(millis)
    }

    #[test]
    fn nothing_is_drawn_until_an_agent_acts() {
        assert_eq!(AgentPointer::default().view(Instant::now()), None);
    }

    #[test]
    fn beginning_a_tool_shows_who_and_what_but_no_pointer_yet() {
        let start = Instant::now();
        let pointer = AgentPointer::default();

        pointer.begin(&who("s1"), "Reading the page", start);

        let view = pointer.view(at(start, 400)).expect("in control");
        assert_eq!(view.harness, "claude");
        assert_eq!(view.label, "Reading the page");
        assert!(view.busy && !view.loading);
        assert_eq!(view.at, None);
        assert_eq!(view.strength, 1.0);
    }

    #[test]
    fn a_finished_tool_leaves_its_label_standing_still() {
        let start = Instant::now();
        let pointer = AgentPointer::default();
        pointer.begin(&who("s1"), "Opening example.com", start);

        pointer.end(at(start, 500));

        let view = pointer.view(at(start, 600)).expect("in control");
        assert_eq!(view.label, "Opening example.com");
        assert!(!view.busy);
    }

    #[test]
    fn the_cursor_is_an_arrow_until_it_becomes_the_text_cursor() {
        let start = Instant::now();
        let pointer = AgentPointer::default();
        pointer.begin(&who("s1"), "", start);
        assert_eq!(
            pointer.view(at(start, 400)).unwrap().cursor,
            CursorShape::Arrow
        );

        pointer.set_cursor(CursorShape::IBeam);

        assert_eq!(
            pointer.view(at(start, 400)).unwrap().cursor,
            CursorShape::IBeam
        );
    }

    #[test]
    fn a_page_load_swaps_the_label_for_a_ring_until_it_ends() {
        let start = Instant::now();
        let pointer = AgentPointer::default();
        pointer.begin(&who("s1"), "Opening example.com", start);

        pointer.set_loading(true);
        assert!(pointer.view(at(start, 10)).unwrap().loading);

        pointer.end(at(start, 900));
        assert!(!pointer.view(at(start, 910)).unwrap().loading);
    }

    #[test]
    fn a_running_tool_keeps_control_however_long_it_takes() {
        let start = Instant::now();
        let pointer = AgentPointer::default();
        pointer.begin(&who("s1"), "Waiting", start);

        assert!(pointer.view(start + Duration::from_secs(25)).is_some());
    }

    #[test]
    fn the_pointer_glides_to_its_target_and_stops_there() {
        let start = Instant::now();
        let pointer = AgentPointer::default();
        pointer.begin(&who("s1"), "Clicking", start);

        let travel = pointer.move_to((400.0, 300.0), start);

        assert!(travel >= MIN_TRAVEL && travel <= MAX_TRAVEL);
        let midway = pointer.view(start + travel / 2).and_then(|v| v.at).unwrap();
        assert!(
            midway.0 < 400.0 && midway.1 < 300.0,
            "on its way: {midway:?}"
        );
        let arrived = pointer.view(start + travel).and_then(|v| v.at).unwrap();
        assert_eq!(arrived, (400.0, 300.0));
    }

    #[test]
    fn the_next_trip_starts_where_the_pointer_is() {
        let start = Instant::now();
        let pointer = AgentPointer::default();
        pointer.begin(&who("s1"), "Clicking", start);
        let first = pointer.move_to((100.0, 100.0), start);
        let later = start + first + Duration::from_millis(50);

        pointer.move_to((500.0, 100.0), later);

        assert_eq!(
            pointer.view(later).and_then(|v| v.at).unwrap(),
            (100.0, 100.0)
        );
    }

    #[test]
    fn a_trip_that_interrupts_a_drift_starts_from_where_the_pointer_is_shown() {
        let start = Instant::now();
        let pointer = AgentPointer::default();
        pointer.begin(&who("s1"), "Scrolling", start);
        let travel = pointer.move_to((200.0, 100.0), start);
        let rest = start + travel;
        pointer.sweep(Sweep::scrolling(true), rest);

        let moment = rest + Duration::from_millis(350);
        let before = pointer.view(moment).and_then(|v| v.at).unwrap();
        assert!(before.1 > 150.0, "drifted down: {before:?}");
        pointer.move_to((500.0, 400.0), moment);

        let after = pointer.view(moment).and_then(|v| v.at).unwrap();
        assert!((after.0 - before.0).abs() < 1e-9 && (after.1 - before.1).abs() < 1e-9);
    }

    #[test]
    fn the_marks_fade_in_instead_of_appearing() {
        let start = Instant::now();
        let pointer = AgentPointer::default();
        pointer.begin(&who("s1"), "Reading the page", start);

        assert_eq!(pointer.view(start).unwrap().strength, 0.0);
        let partway = pointer.view(at(start, 125)).unwrap().strength;
        assert!(partway > 0.0 && partway < 1.0);
        assert_eq!(pointer.view(at(start, 400)).unwrap().strength, 1.0);
    }

    #[test]
    fn scrolling_up_drifts_the_pointer_up() {
        let start = Instant::now();
        let pointer = AgentPointer::default();
        pointer.begin(&who("s1"), "Scrolling", start);
        let travel = pointer.move_to((200.0, 300.0), start);
        let rest = start + travel;

        pointer.sweep(Sweep::scrolling(false), rest);

        let during = pointer
            .view(rest + Duration::from_millis(350))
            .and_then(|v| v.at)
            .unwrap();
        assert!(during.1 < 300.0, "{during:?}");
    }

    #[test]
    fn a_click_makes_a_ripple_that_ends() {
        let start = Instant::now();
        let pointer = AgentPointer::default();
        pointer.begin(&who("s1"), "Clicking", start);
        pointer.move_to((10.0, 10.0), start);

        pointer.click(at(start, 700));

        assert!(pointer.view(at(start, 800)).unwrap().ripple.is_some());
        assert_eq!(pointer.view(at(start, 1300)).unwrap().ripple, None);
    }

    #[test]
    fn control_fades_and_then_ends_when_the_agent_goes_quiet() {
        let start = Instant::now();
        let pointer = AgentPointer::default();
        pointer.begin(&who("s1"), "Reading", start);
        pointer.end(start);

        let fading = pointer.view(start + CONTROL_HOLD - Duration::from_millis(300));
        assert!(fading.unwrap().strength < 1.0);
        assert_eq!(pointer.view(start + CONTROL_HOLD), None);
    }

    #[test]
    fn control_lasts_longer_while_the_agents_session_works() {
        let start = Instant::now();
        let pointer = AgentPointer::default();
        pointer.begin(&who("s1"), "Reading", start);
        pointer.end(start);
        pointer.session_status("s1", true, start);

        assert!(pointer.view(start + Duration::from_secs(20)).is_some());
        assert_eq!(pointer.view(start + Duration::from_secs(31)), None);
    }

    #[test]
    fn control_winds_down_soon_after_the_session_stops_working() {
        let start = Instant::now();
        let pointer = AgentPointer::default();
        pointer.begin(&who("s1"), "Reading", start);
        pointer.end(start);
        pointer.session_status("s1", true, start);

        pointer.session_status("s1", false, at(start, 2000));

        assert!(pointer.view(at(start, 3000)).is_some());
        assert_eq!(pointer.view(at(start, 3600)), None);
    }

    #[test]
    fn another_sessions_status_changes_nothing() {
        let start = Instant::now();
        let pointer = AgentPointer::default();
        pointer.begin(&who("s1"), "Reading", start);
        pointer.end(start);

        pointer.session_status("s2", false, start);

        assert!(pointer.view(at(start, 4000)).is_some());
    }

    #[test]
    fn another_agent_takes_over_the_label_and_the_icon() {
        let start = Instant::now();
        let pointer = AgentPointer::default();
        pointer.begin(&who("s1"), "Reading", start);

        let codex = Controller {
            session: "s2".to_owned(),
            harness: "codex".to_owned(),
        };
        pointer.begin(&codex, "Typing", at(start, 5000));

        let view = pointer.view(at(start, 5100)).expect("in control");
        assert_eq!(
            (view.harness.as_str(), view.label.as_str()),
            ("codex", "Typing")
        );
    }

    #[test]
    fn release_ends_control_at_once() {
        let start = Instant::now();
        let pointer = AgentPointer::default();
        pointer.begin(&who("s1"), "Reading", start);

        pointer.release();

        assert_eq!(pointer.view(start), None);
    }

    #[test]
    fn the_pointer_counts_as_placed_only_once_it_has_gone_somewhere() {
        let start = Instant::now();
        let pointer = AgentPointer::default();
        pointer.begin(&who("s1"), "Reading", start);
        assert!(!pointer.is_placed());

        pointer.move_to((1.0, 1.0), start);
        assert!(pointer.is_placed());
    }
}
