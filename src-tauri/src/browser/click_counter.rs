//! Counts consecutive clicks (single, double, triple). GTK reports double and triple clicks as
//! extra events on top of the plain press, so the count is kept here instead and sent with each press.

use std::time::Duration;

/// How close in time and space presses must be to count as one series.
const WINDOW: Duration = Duration::from_millis(400);
const DISTANCE: i32 = 4;

#[derive(Default)]
pub struct ClickCounter {
    last: Option<Press>,
    count: u8,
}

struct Press {
    at: Duration,
    x: i32,
    y: i32,
    button: u32,
}

impl ClickCounter {
    /// Registers a press and returns which one in its series it is, from 1 to 3.
    pub fn press(&mut self, at: Duration, x: i32, y: i32, button: u32) -> u8 {
        let continues = self.last.as_ref().is_some_and(|last| {
            last.button == button
                && at.saturating_sub(last.at) <= WINDOW
                && (x - last.x).abs() <= DISTANCE
                && (y - last.y).abs() <= DISTANCE
        });
        self.count = if continues { self.count % 3 + 1 } else { 1 };
        self.last = Some(Press { at, x, y, button });
        self.count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(ms: u64) -> Duration {
        Duration::from_millis(ms)
    }

    #[test]
    fn quick_presses_in_place_count_up_to_three_then_start_over() {
        let mut counter = ClickCounter::default();
        assert_eq!(counter.press(at(0), 10, 10, 1), 1);
        assert_eq!(counter.press(at(200), 11, 10, 1), 2);
        assert_eq!(counter.press(at(400), 10, 11, 1), 3);
        assert_eq!(counter.press(at(600), 10, 10, 1), 1);
    }

    #[test]
    fn a_pause_starts_a_new_series() {
        let mut counter = ClickCounter::default();
        counter.press(at(0), 10, 10, 1);
        assert_eq!(counter.press(at(1000), 10, 10, 1), 1);
    }

    #[test]
    fn moving_away_starts_a_new_series() {
        let mut counter = ClickCounter::default();
        counter.press(at(0), 10, 10, 1);
        assert_eq!(counter.press(at(100), 60, 10, 1), 1);
    }

    #[test]
    fn another_button_starts_a_new_series() {
        let mut counter = ClickCounter::default();
        counter.press(at(0), 10, 10, 1);
        assert_eq!(counter.press(at(100), 10, 10, 3), 1);
    }
}
