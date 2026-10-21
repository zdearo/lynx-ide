//! Mouse, keyboard and text: protocol commands turned into engine input events.

use cef::*;
use lynx_browser_protocol::message::{KeyKind, MouseButton, PageCommand};

pub fn apply(host: &BrowserHost, command: PageCommand) {
    match command {
        PageCommand::MouseMove {
            x,
            y,
            modifiers,
            leave,
        } => {
            host.send_mouse_move_event(Some(&MouseEvent { x, y, modifiers }), i32::from(leave));
        }
        PageCommand::MouseClick {
            x,
            y,
            button,
            pressed,
            clicks,
            modifiers,
        } => {
            let button = match button {
                MouseButton::Left => MouseButtonType::LEFT,
                MouseButton::Middle => MouseButtonType::MIDDLE,
                MouseButton::Right => MouseButtonType::RIGHT,
            };
            host.send_mouse_click_event(
                Some(&MouseEvent { x, y, modifiers }),
                button,
                i32::from(!pressed),
                i32::from(clicks.max(1)),
            );
        }
        PageCommand::MouseWheel {
            x,
            y,
            delta_x,
            delta_y,
            modifiers,
        } => {
            host.send_mouse_wheel_event(Some(&MouseEvent { x, y, modifiers }), delta_x, delta_y);
        }
        PageCommand::Key {
            kind,
            windows_code,
            native_code,
            modifiers,
            character,
        } => {
            let event = |type_| KeyEvent {
                type_,
                modifiers,
                windows_key_code: windows_code,
                native_key_code: native_code,
                character,
                unmodified_character: character,
                ..Default::default()
            };
            match kind {
                KeyKind::Down => {
                    host.send_key_event(Some(&event(KeyEventType::RAWKEYDOWN)));
                    if character != 0 {
                        host.send_key_event(Some(&event(KeyEventType::CHAR)));
                    }
                }
                KeyKind::Up => host.send_key_event(Some(&event(KeyEventType::KEYUP))),
            }
        }
        PageCommand::Text { text } => type_text(host, &text),
        // Everything else is handled by the engine module, which never forwards it here.
        _ => {}
    }
}

/// Types text the way the engine's own sample does: one `CHAR` event per character. That covers
/// accents and composed letters, which are single characters. Characters outside the basic plane
/// (emoji) are two UTF-16 units, which a `CHAR` event cannot carry, so those go to the input method.
fn type_text(host: &BrowserHost, text: &str) {
    if text.chars().any(|character| character.len_utf16() > 1) {
        host.ime_commit_text(Some(&CefString::from(text)), None, 0);
        return;
    }
    for unit in text.encode_utf16() {
        let event = KeyEvent {
            type_: KeyEventType::CHAR,
            windows_key_code: i32::from(unit),
            character: unit,
            unmodified_character: unit,
            ..Default::default()
        };
        host.send_key_event(Some(&event));
    }
}
