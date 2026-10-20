//! Keyboard translation: GTK key symbols (X11 keysyms) to the Windows virtual-key codes the
//! engine expects. Pure, so it can be tested without a display.

const SPACE: u32 = 0x20;
const BACKSPACE: u32 = 0xff08;
const TAB: u32 = 0xff09;
const RETURN: u32 = 0xff0d;
const ESCAPE: u32 = 0xff1b;
const HOME: u32 = 0xff50;
const LEFT: u32 = 0xff51;
const UP: u32 = 0xff52;
const RIGHT: u32 = 0xff53;
const DOWN: u32 = 0xff54;
const PAGE_UP: u32 = 0xff55;
const PAGE_DOWN: u32 = 0xff56;
const END: u32 = 0xff57;
const INSERT: u32 = 0xff63;
const KP_ENTER: u32 = 0xff8d;
const F1: u32 = 0xffbe;
const F12: u32 = 0xffc9;
const SHIFT_L: u32 = 0xffe1;
const SHIFT_R: u32 = 0xffe2;
const CONTROL_L: u32 = 0xffe3;
const CONTROL_R: u32 = 0xffe4;
const ALT_L: u32 = 0xffe9;
const ALT_R: u32 = 0xffea;
const DELETE: u32 = 0xffff;

/// The Windows virtual-key code for a key symbol, or 0 when there is no equivalent.
pub fn windows_key_code(keysym: u32) -> i32 {
    match keysym {
        BACKSPACE => 8,
        TAB => 9,
        RETURN | KP_ENTER => 13,
        ESCAPE => 27,
        SPACE => 32,
        PAGE_UP => 33,
        PAGE_DOWN => 34,
        END => 35,
        HOME => 36,
        LEFT => 37,
        UP => 38,
        RIGHT => 39,
        DOWN => 40,
        INSERT => 45,
        DELETE => 46,
        SHIFT_L | SHIFT_R => 16,
        CONTROL_L | CONTROL_R => 17,
        ALT_L | ALT_R => 18,
        F1..=F12 => 112 + (keysym - F1) as i32,
        // Digits and letters have the code of their upper-case ASCII character.
        0x30..=0x39 => keysym as i32,
        0x41..=0x5a => keysym as i32,
        0x61..=0x7a => keysym as i32 - 0x20,
        // Punctuation on a US layout.
        0x3b | 0x3a => 186, // ; :
        0x3d | 0x2b => 187, // = +
        0x2c | 0x3c => 188, // , <
        0x2d | 0x5f => 189, // - _
        0x2e | 0x3e => 190, // . >
        0x2f | 0x3f => 191, // / ?
        0x60 | 0x7e => 192, // ` ~
        0x5b | 0x7b => 219, // [ {
        0x5c | 0x7c => 220, // \ |
        0x5d | 0x7d => 221, // ] }
        0x27 | 0x22 => 222, // ' "
        _ => 0,
    }
}

/// The control character a key produces by itself, which the engine needs as a separate event
/// for forms to submit and fields to take a tab or a backspace. Printable characters are
/// delivered as text instead.
pub fn control_character(keysym: u32) -> u16 {
    match keysym {
        BACKSPACE => 8,
        TAB => 9,
        RETURN | KP_ENTER => 13,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_have_the_code_of_the_upper_case_letter() {
        assert_eq!(windows_key_code('a' as u32), 65);
        assert_eq!(windows_key_code('A' as u32), 65);
        assert_eq!(windows_key_code('z' as u32), 90);
    }

    #[test]
    fn digits_keep_their_ascii_code() {
        assert_eq!(windows_key_code('0' as u32), 48);
        assert_eq!(windows_key_code('9' as u32), 57);
    }

    #[test]
    fn navigation_and_editing_keys() {
        assert_eq!(windows_key_code(RETURN), 13);
        assert_eq!(windows_key_code(KP_ENTER), 13);
        assert_eq!(windows_key_code(BACKSPACE), 8);
        assert_eq!(windows_key_code(DELETE), 46);
        assert_eq!(windows_key_code(LEFT), 37);
        assert_eq!(windows_key_code(DOWN), 40);
        assert_eq!(windows_key_code(PAGE_DOWN), 34);
        assert_eq!(windows_key_code(ESCAPE), 27);
    }

    #[test]
    fn function_keys_run_from_112() {
        assert_eq!(windows_key_code(F1), 112);
        assert_eq!(windows_key_code(F12), 123);
    }

    #[test]
    fn shifted_and_plain_punctuation_share_a_code() {
        assert_eq!(windows_key_code('-' as u32), windows_key_code('_' as u32));
        assert_eq!(windows_key_code('/' as u32), 191);
        assert_eq!(windows_key_code(';' as u32), windows_key_code(':' as u32));
    }

    #[test]
    fn a_key_with_no_equivalent_is_zero() {
        assert_eq!(windows_key_code(0x10000e7), 0); // a symbol outside the table
        assert_eq!(windows_key_code(0xffff_fff0), 0);
    }

    #[test]
    fn only_a_few_keys_produce_a_control_character() {
        assert_eq!(control_character(RETURN), 13);
        assert_eq!(control_character(TAB), 9);
        assert_eq!(control_character(BACKSPACE), 8);
        assert_eq!(control_character('a' as u32), 0);
        assert_eq!(control_character(LEFT), 0);
    }
}
