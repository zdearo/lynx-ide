//! The terminal state a replay has to set before its text, read from the output the replay drops.
//!
//! The output the scrollback evicts is gone, but what it set is not: bracketed paste, a hidden cursor,
//! the alternate screen, a background color that stays on. `ReplayState` reads those from every byte it
//! is fed, and `preamble` writes them back as escape sequences in front of the text that is left.
//!
//! The parser follows the VT state machine: a sequence split across two chunks is read whole, and a
//! line feed inside a sequence does not end it.
//!
//! shortcut: DEC charsets (`ESC (`, SO/SI), scroll margins (DECSTBM), DECSC/DECRC, keypad mode
//! (`ESC =`, `ESC >`), kitty keyboard flags (`CSI > u`), DECSTR (`CSI ! p`) and ANSI modes 4 and 20
//! are not read. A replay that drops one of them shows the wrong charset, margins or keys until the
//! program sets it again. When one of them shows up, read it here the same way: in `csi_final` for
//! sequences and in `preamble` for what is written back.

use std::collections::BTreeMap;
use std::fmt::Write as _;

const ESC: u8 = 0x1b;
const BEL: u8 = 0x07;
const CAN: u8 = 0x18;
const SUB: u8 = 0x1a;

/// Longest CSI body or OSC payload read. A longer one is dropped, and the state it would set stays as it was.
/// shortcut: an OSC 8 link longer than this is not replayed, raise it if a link is cut short after a replay.
const MAX_STRING_BYTES: usize = 4096;
/// Private modes that are not replayed. 2026 frames one redraw and 1048 saves the cursor, so neither is a
/// state to keep. 3 (DECCOLM) switches the column count, which the view sets on its own.
const NOT_REPLAYED_MODES: [u16; 3] = [3, 1048, 2026];
/// The modes a fresh terminal starts with set: a reset of one of them has to be replayed.
const STARTS_SET: [u16; 2] = [7, 25];
/// Entering the alternate screen saves the cursor and switches buffers, so it goes first in the preamble.
const ALTERNATE_SCREEN: [u16; 3] = [1049, 1047, 47];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Stage {
    #[default]
    Ground,
    /// After ESC.
    Escape,
    /// After ESC and an intermediate byte; the final byte that follows ends it.
    EscapeIntermediate,
    /// Inside CSI: parameters and intermediates, until the final byte.
    Csi,
    /// Inside an OSC (`osc`) or another string (DCS, SOS, PM, APC), until its end.
    Text { osc: bool },
    /// After ESC inside a string: `\` completes the string terminator.
    StringEscape,
}

/// The state a terminal reads from output, enough to set it again on a fresh terminal.
#[derive(Clone, Debug, Default)]
pub struct ReplayState {
    stage: Stage,
    /// The CSI body or the OSC payload being read.
    buffer: Vec<u8>,
    /// Set when the body or payload outgrew `MAX_STRING_BYTES`: it is dropped unread.
    overflow: bool,
    /// Private modes last set or reset, by number.
    modes: BTreeMap<u16, bool>,
    /// SGR attributes in effect, one raw parameter group per attribute class.
    attributes: BTreeMap<&'static str, String>,
    /// The OSC 8 hyperlink open, as the raw `params;uri` of the sequence that opened it.
    hyperlink: Option<String>,
    /// The DECSCUSR cursor style a program set.
    cursor_style: Option<u8>,
}

impl ReplayState {
    /// Reads output as the terminal does, from the state the output before it left.
    pub fn feed(&mut self, bytes: &[u8]) {
        let mut index = 0;
        while index < bytes.len() {
            if self.is_ground() {
                // Text changes nothing in ground, so only the next ESC needs reading.
                match bytes[index..].iter().position(|&byte| byte == ESC) {
                    Some(found) => index += found,
                    None => break,
                }
            }
            self.read(bytes[index]);
            index += 1;
        }
    }

    /// Reads one byte of output.
    fn read(&mut self, byte: u8) {
        self.stage = match self.stage {
            _ if matches!(byte, CAN | SUB) => Stage::Ground,
            Stage::Ground if byte == ESC => Stage::Escape,
            Stage::Ground => Stage::Ground,
            Stage::Escape => self.escape(byte),
            Stage::EscapeIntermediate if (0x30..=0x7e).contains(&byte) => Stage::Ground,
            Stage::EscapeIntermediate => Stage::EscapeIntermediate,
            Stage::Csi if byte == ESC => Stage::Escape,
            Stage::Csi => self.csi(byte),
            Stage::Text { osc: true } if byte == BEL => {
                self.end_string();
                Stage::Ground
            }
            Stage::Text { osc } if byte == ESC => {
                if osc {
                    self.end_string();
                }
                Stage::StringEscape
            }
            Stage::Text { osc: true } => {
                self.collect(byte);
                Stage::Text { osc: true }
            }
            Stage::Text { osc: false } => Stage::Text { osc: false },
            Stage::StringEscape if byte == b'\\' => Stage::Ground,
            Stage::StringEscape => self.escape(byte),
        };
    }

    /// True between sequences. A replay cut here starts outside any sequence.
    pub fn is_ground(&self) -> bool {
        self.stage == Stage::Ground
    }

    /// The escape sequences that set this state on a fresh terminal, written before the replayed text.
    pub fn preamble(&self) -> String {
        let mut out = String::new();
        for number in ALTERNATE_SCREEN {
            if self.modes.get(&number) == Some(&true) {
                let _ = write!(out, "\x1b[?{number}h");
            }
        }
        for (&number, &on) in &self.modes {
            if ALTERNATE_SCREEN.contains(&number) {
                continue;
            }
            if on {
                let _ = write!(out, "\x1b[?{number}h");
            } else if STARTS_SET.contains(&number) {
                let _ = write!(out, "\x1b[?{number}l");
            }
        }
        if let Some(link) = &self.hyperlink {
            let _ = write!(out, "\x1b]8;{link}\x1b\\");
        }
        if !self.attributes.is_empty() {
            let values: Vec<&str> = self.attributes.values().map(String::as_str).collect();
            let _ = write!(out, "\x1b[0;{}m", values.join(";"));
        }
        if let Some(style) = self.cursor_style {
            let _ = write!(out, "\x1b[{style} q");
        }
        out
    }

    fn reset(&mut self) {
        *self = Self::default();
    }

    fn escape(&mut self, byte: u8) -> Stage {
        match byte {
            ESC => Stage::Escape,
            b'[' => {
                self.buffer.clear();
                self.overflow = false;
                Stage::Csi
            }
            b']' => {
                self.buffer.clear();
                self.overflow = false;
                Stage::Text { osc: true }
            }
            b'P' | b'X' | b'^' | b'_' => Stage::Text { osc: false },
            0x20..=0x2f => Stage::EscapeIntermediate,
            0x30..=0x7e => {
                // RIS: a full reset of the terminal.
                if byte == b'c' {
                    self.reset();
                }
                Stage::Ground
            }
            // C0 controls run inside an escape and leave it open.
            _ => Stage::Escape,
        }
    }

    fn csi(&mut self, byte: u8) -> Stage {
        match byte {
            0x20..=0x3f => {
                self.collect(byte);
                Stage::Csi
            }
            0x40..=0x7e => {
                let body = std::mem::take(&mut self.buffer);
                if !std::mem::take(&mut self.overflow) {
                    self.csi_final(byte, &body);
                }
                // Hand the buffer back: the next sequence reuses its capacity.
                self.buffer = body;
                self.buffer.clear();
                Stage::Ground
            }
            // C0 controls run inside a sequence and leave it open.
            _ => Stage::Csi,
        }
    }

    fn collect(&mut self, byte: u8) {
        if self.buffer.len() < MAX_STRING_BYTES {
            self.buffer.push(byte);
        } else {
            self.overflow = true;
        }
    }

    fn csi_final(&mut self, last: u8, body: &[u8]) {
        let (private, params_and_intermediates) = match body.split_first() {
            Some((b'?', rest)) => (true, rest),
            _ => (false, body),
        };
        let split = params_and_intermediates
            .iter()
            .position(|byte| (0x20..=0x2f).contains(byte))
            .unwrap_or(params_and_intermediates.len());
        let (params, intermediates) = params_and_intermediates.split_at(split);
        // Other private markers (`<`, `=`, `>`) belong to other sequences, such as xterm's key modes.
        if !params
            .iter()
            .all(|byte| byte.is_ascii_digit() || *byte == b';' || *byte == b':')
        {
            return;
        }
        match (private, last, intermediates) {
            (true, b'h' | b'l', []) => self.set_modes(params, last == b'h'),
            (false, b'm', []) => self.set_attributes(params),
            (false, b'q', [b' ']) => {
                let style = if params.is_empty() {
                    Some(0)
                } else {
                    parse_number(params).and_then(|style| u8::try_from(style).ok())
                };
                if let Some(style) = style {
                    self.cursor_style = Some(style);
                }
            }
            _ => {}
        }
    }

    fn set_modes(&mut self, params: &[u8], on: bool) {
        for group in params.split(|byte| *byte == b';') {
            let Some(number) = parse_number(group) else {
                continue;
            };
            if !NOT_REPLAYED_MODES.contains(&number) {
                self.modes.insert(number, on);
            }
        }
    }

    /// SGR: each parameter sets or clears one attribute class. A color keeps its raw parameters, so
    /// `38;5;208` and `38;2;r;g;b` are one color, and `4:3` keeps its underline style.
    fn set_attributes(&mut self, params: &[u8]) {
        let Ok(text) = std::str::from_utf8(params) else {
            return;
        };
        if text.is_empty() {
            self.attributes.clear();
            return;
        }
        let mut groups = text.split(';');
        while let Some(group) = groups.next() {
            let code = group
                .split(':')
                .next()
                .and_then(|code| code.parse::<u16>().ok());
            match code.unwrap_or(0) {
                0 => self.attributes.clear(),
                1 => self.set("bold", group),
                2 => self.set("dim", group),
                3 => self.set("italic", group),
                4 if group.split(':').nth(1) == Some("0") => self.clear("underline"),
                4 | 21 => self.set("underline", group),
                5 => self.set("blink", group),
                7 => self.set("inverse", group),
                8 => self.set("invisible", group),
                9 => self.set("strike", group),
                22 => {
                    self.clear("bold");
                    self.clear("dim");
                }
                23 => self.clear("italic"),
                24 => self.clear("underline"),
                25 => self.clear("blink"),
                27 => self.clear("inverse"),
                28 => self.clear("invisible"),
                29 => self.clear("strike"),
                30..=37 | 90..=97 => self.set("fg", group),
                39 => self.clear("fg"),
                40..=47 | 100..=107 => self.set("bg", group),
                49 => self.clear("bg"),
                53 => self.set("overline", group),
                55 => self.clear("overline"),
                code @ (38 | 48 | 58) => {
                    let class = match code {
                        38 => "fg",
                        48 => "bg",
                        _ => "underline color",
                    };
                    if group.contains(':') {
                        self.set(class, group);
                    } else {
                        // `38;5;n` takes two more parameters and `38;2;r;g;b` takes four.
                        let extra = match groups.clone().next() {
                            Some("5") => 2,
                            Some("2") => 4,
                            _ => 0,
                        };
                        let mut value = group.to_owned();
                        for part in groups.by_ref().take(extra) {
                            value.push(';');
                            value.push_str(part);
                        }
                        self.set(class, &value);
                    }
                }
                59 => self.clear("underline color"),
                _ => {}
            }
        }
    }

    /// Replaces a class's value, reusing its string: a color changes many times in a session.
    fn set(&mut self, class: &'static str, value: &str) {
        match self.attributes.get_mut(class) {
            Some(existing) => {
                existing.clear();
                existing.push_str(value);
            }
            None => {
                self.attributes.insert(class, value.to_owned());
            }
        }
    }

    fn clear(&mut self, class: &'static str) {
        self.attributes.remove(class);
    }

    /// OSC 8 opens a hyperlink with `8;params;uri` and closes it with an empty uri. Other strings
    /// (titles, colors, clipboard) do not change what a replay has to set.
    fn end_string(&mut self) {
        let payload = std::mem::take(&mut self.buffer);
        if std::mem::take(&mut self.overflow) {
            return;
        }
        let Some(link) = payload.strip_prefix(b"8;") else {
            return;
        };
        let Ok(link) = std::str::from_utf8(link) else {
            return;
        };
        let Some((_, uri)) = link.split_once(';') else {
            return;
        };
        self.hyperlink = if uri.is_empty() {
            None
        } else {
            Some(link.to_owned())
        };
    }
}

fn parse_number(digits: &[u8]) -> Option<u16> {
    std::str::from_utf8(digits).ok()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preamble_of(output: &str) -> String {
        let mut state = ReplayState::default();
        state.feed(output.as_bytes());
        state.preamble()
    }

    #[test]
    fn a_mode_that_is_set_is_replayed_set() {
        assert_eq!(preamble_of("\x1b[?2004h"), "\x1b[?2004h");
    }

    #[test]
    fn a_mode_reset_is_replayed_only_when_a_fresh_terminal_starts_it_set() {
        assert_eq!(preamble_of("\x1b[?2004h\x1b[?2004l"), "");
        assert_eq!(preamble_of("\x1b[?25l"), "\x1b[?25l");
        assert_eq!(preamble_of("\x1b[?7l"), "\x1b[?7l");
    }

    #[test]
    fn the_alternate_screen_comes_first_in_the_preamble() {
        assert_eq!(
            preamble_of("\x1b[?2004h\x1b[?1000h\x1b[?1049h"),
            "\x1b[?1049h\x1b[?1000h\x1b[?2004h"
        );
    }

    #[test]
    fn modes_that_are_not_states_are_not_replayed() {
        assert_eq!(preamble_of("\x1b[?2026h\x1b[?1048h\x1b[?3h"), "");
    }

    #[test]
    fn a_sequence_split_across_feeds_is_read_whole() {
        let mut state = ReplayState::default();
        state.feed(b"\x1b[?20");
        assert!(!state.is_ground());
        state.feed(b"04h");
        assert!(state.is_ground());
        assert_eq!(state.preamble(), "\x1b[?2004h");
    }

    #[test]
    fn a_line_feed_inside_a_sequence_is_executed_and_the_sequence_goes_on() {
        let mut state = ReplayState::default();
        state.feed(b"\x1b[?25\n");
        assert!(!state.is_ground());
        state.feed(b"l");
        assert_eq!(state.preamble(), "\x1b[?25l");
    }

    #[test]
    fn attributes_in_effect_are_replayed_and_a_class_is_cleared_by_its_off_code() {
        assert_eq!(preamble_of("\x1b[1;38;5;208m\x1b[22m"), "\x1b[0;38;5;208m");
        assert_eq!(preamble_of("\x1b[44m\x1b[49m"), "");
    }

    #[test]
    fn a_reset_clears_every_attribute() {
        assert_eq!(preamble_of("\x1b[44;1m\x1b[0m"), "");
        assert_eq!(preamble_of("\x1b[44m\x1b[m"), "");
    }

    #[test]
    fn a_true_color_is_one_color_and_its_numbers_are_not_codes() {
        assert_eq!(preamble_of("\x1b[38;2;0;44;0m"), "\x1b[0;38;2;0;44;0m");
    }

    #[test]
    fn a_colon_form_keeps_its_arguments_and_its_underline_style() {
        assert_eq!(preamble_of("\x1b[4:3m"), "\x1b[0;4:3m");
        assert_eq!(preamble_of("\x1b[4:3m\x1b[4:0m"), "");
    }

    #[test]
    fn a_hyperlink_that_is_open_is_replayed_and_a_closed_one_is_not() {
        assert_eq!(
            preamble_of("\x1b]8;;https://example.com\x07"),
            "\x1b]8;;https://example.com\x1b\\"
        );
        assert_eq!(
            preamble_of("\x1b]8;id=a;https://example.com\x1b\\\x1b]8;;\x1b\\"),
            ""
        );
    }

    #[test]
    fn a_title_that_looks_like_a_mode_is_only_text() {
        assert_eq!(preamble_of("\x1b]0;[?25l title\x07"), "");
    }

    #[test]
    fn the_cursor_style_is_replayed() {
        assert_eq!(preamble_of("\x1b[6 q"), "\x1b[6 q");
    }

    #[test]
    fn other_private_markers_are_not_read_as_attributes() {
        // xterm's key modes: `CSI > 4 ; 2 m` is not SGR.
        assert_eq!(preamble_of("\x1b[>4;2m"), "");
    }

    #[test]
    fn a_control_byte_cancels_the_sequence_being_read() {
        let mut state = ReplayState::default();
        state.feed(b"\x1b[?25\x18");
        assert!(state.is_ground());
        assert_eq!(state.preamble(), "");
    }

    #[test]
    fn an_overlong_sequence_is_dropped_and_leaves_the_state_as_it_was() {
        let mut state = ReplayState::default();
        state.feed(b"\x1b[?2004h");
        let mut long = b"\x1b[?25;".to_vec();
        long.extend(std::iter::repeat_n(b'1', MAX_STRING_BYTES));
        long.push(b'l');
        state.feed(&long);
        assert_eq!(state.preamble(), "\x1b[?2004h");
    }
}
