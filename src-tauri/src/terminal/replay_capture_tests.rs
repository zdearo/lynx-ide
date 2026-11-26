//! Real program output as regression for the attach replay. Each fixture is a PTY recording of a
//! program, and each one needs a different piece of state at the attach: a full-screen app in the
//! alternate screen, an Ink-style app that hides the cursor and turns on bracketed paste, and a
//! background color that stays set through the lines.
//!
//! The fixtures were checked against xterm.js, the page's terminal, with the whole output and with
//! the replay: the screens, cursors, modes and attributes matched.

use super::scrollback_buffer::ScrollbackBuffer;

const TUI_ALTERNATE_SCREEN: &[u8] = include_bytes!("replay_fixtures/tui_alternate_screen.bin");
const INK_MAIN_SCREEN: &[u8] = include_bytes!("replay_fixtures/ink_main_screen.bin");
const BACKGROUND_PERSISTS: &[u8] = include_bytes!("replay_fixtures/background_persists.bin");
/// Claude Code's classic renderer (`CLAUDE_CODE_DISABLE_ALTERNATE_SCREEN=1`), with output above its frame.
/// Its frames move the cursor relatively, so they draw right only from the first byte of the output.
const CLAUDE_MAIN_SCREEN: &[u8] = include_bytes!("replay_fixtures/claude_main_screen.bin");

/// The output in the pieces the reader hands the buffer: up to 16 KiB, cut between characters.
fn buffer_of(capture: &str) -> ScrollbackBuffer {
    let mut buffer = ScrollbackBuffer::new(1024 * 1024);
    let mut start = 0;
    while start < capture.len() {
        let mut end = (start + 16 * 1024).min(capture.len());
        while !capture.is_char_boundary(end) {
            end += 1;
        }
        buffer.push(capture[start..end].to_owned());
        start = end;
    }
    buffer
}

fn text(capture: &[u8]) -> &str {
    std::str::from_utf8(capture).expect("a capture is UTF-8")
}

/// The fixtures are under the buffer's capacity, so nothing is evicted and the replay is the output itself.
#[test]
fn each_capture_is_replayed_whole() {
    for capture in [TUI_ALTERNATE_SCREEN, INK_MAIN_SCREEN, BACKGROUND_PERSISTS] {
        let full = text(capture);
        assert_eq!(buffer_of(full).replay(), full);
    }
}

#[test]
fn claude_main_screen_is_replayed_from_its_first_byte() {
    // A replay that started anywhere else would draw the relative moves of the frames in the wrong rows.
    let full = text(CLAUDE_MAIN_SCREEN);
    assert_eq!(buffer_of(full).replay(), full);
}

#[test]
fn a_full_screen_app_keeps_its_alternate_screen_and_mouse_reporting() {
    let replay = buffer_of(text(TUI_ALTERNATE_SCREEN)).replay();

    assert!(replay.starts_with("\x1b[?1049h"));
    assert!(replay.contains("\x1b[?1000h"), "mouse reporting stays on");
    assert!(replay.contains("\x1b[?1006h"));
}

#[test]
fn an_ink_style_app_keeps_bracketed_paste_and_the_hidden_cursor() {
    let replay = buffer_of(text(INK_MAIN_SCREEN)).replay();

    assert!(replay.contains("\x1b[?2004h"));
    assert!(replay.contains("\x1b[?25l"));
}

#[test]
fn a_background_color_that_stays_set_is_kept_with_the_text_it_paints() {
    let replay = buffer_of(text(BACKGROUND_PERSISTS)).replay();

    assert!(replay.contains("\x1b[44mblue-band line 00000"));
}
