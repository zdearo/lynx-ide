//! Messages on the socket: one JSON object per line, in each direction.
//!
//! Both sides are programs we ship together, so the format is not versioned yet; when the engine
//! is installed separately from the app, a version handshake has to come first.

use std::io::{self, BufRead, Write};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// Bumped whenever the messages change in a way an older host or app cannot follow. The installed
/// engine records the version it speaks, and the app refuses one that speaks another.
pub const PROTOCOL_VERSION: u32 = 2;

/// Modifier bits. They have the values CEF uses for its event flags, so the host passes them on.
pub mod modifiers {
    pub const SHIFT: u32 = 1 << 1;
    pub const CONTROL: u32 = 1 << 2;
    pub const ALT: u32 = 1 << 3;
    pub const LEFT_BUTTON: u32 = 1 << 4;
    pub const MIDDLE_BUTTON: u32 = 1 << 5;
    pub const RIGHT_BUTTON: u32 = 1 << 6;
    pub const COMMAND: u32 = 1 << 7;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MouseButton {
    Left,
    Middle,
    Right,
}

/// An editing action the page cannot get from key events alone: an engine that draws off-screen
/// has no system key bindings behind it, so Ctrl+C and the like do nothing unless they are asked
/// for by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EditCommand {
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
}

impl EditCommand {
    /// The command a shortcut stands for, if any: Ctrl (or Command) with C, X, V, A, Z, or Y, and
    /// Shift+Z for redo. `windows_code` is the key's Windows virtual-key code, so letters are
    /// their capital's ASCII value.
    pub fn from_shortcut(modifiers: u32, windows_code: i32) -> Option<Self> {
        let primary = modifiers & (modifiers::CONTROL | modifiers::COMMAND) != 0;
        let alt = modifiers & modifiers::ALT != 0;
        if !primary || alt {
            return None;
        }
        let shift = modifiers & modifiers::SHIFT != 0;
        match u8::try_from(windows_code).ok()? {
            b'C' if !shift => Some(Self::Copy),
            b'X' if !shift => Some(Self::Cut),
            b'V' if !shift => Some(Self::Paste),
            b'A' if !shift => Some(Self::SelectAll),
            b'Z' if shift => Some(Self::Redo),
            b'Z' => Some(Self::Undo),
            b'Y' if !shift => Some(Self::Redo),
            _ => None,
        }
    }
}

/// What the engine did on its own because nobody was there to ask.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NoticeKind {
    /// A JavaScript dialog (`alert`, `confirm`, `prompt`, leaving a page) was answered.
    Dialog,
    /// The page asked for a permission (camera, location...) and was refused.
    Permission,
    /// A download was blocked.
    Download,
    /// The page asked for a file chooser, and none was shown.
    FileChooser,
    /// A new window was opened in the page itself.
    Popup,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KeyKind {
    Down,
    Up,
}

/// From Lynx to the host. Coordinates are in view pixels (logical), not device pixels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ToHost {
    Navigate {
        url: String,
    },
    Reload {
        ignore_cache: bool,
    },
    Back,
    Forward,
    Stop,
    /// The view's size in logical pixels and the density of the screen it is shown on. Frames
    /// come back at `width * scale` by `height * scale` device pixels.
    Resize {
        width: u32,
        height: u32,
        scale: f32,
    },
    Visible {
        visible: bool,
    },
    Focus {
        focused: bool,
    },
    MouseMove {
        x: i32,
        y: i32,
        modifiers: u32,
        leave: bool,
    },
    MouseClick {
        x: i32,
        y: i32,
        button: MouseButton,
        pressed: bool,
        clicks: u8,
        modifiers: u32,
    },
    MouseWheel {
        x: i32,
        y: i32,
        delta_x: i32,
        delta_y: i32,
        modifiers: u32,
    },
    /// A key that is not text. `character` is set for the few that also produce a control
    /// character (Enter, Tab, Backspace); typed text goes through [`ToHost::Text`] instead.
    Key {
        kind: KeyKind,
        windows_code: i32,
        native_code: i32,
        modifiers: u32,
        character: u16,
    },
    /// A cut, copy, paste, undo, redo or select-all, for the focused frame.
    Edit {
        command: EditCommand,
    },
    /// Text the input method committed: letters, accents, composed characters.
    Text {
        text: String,
    },
    /// Lynx finished copying a frame, so the host may reuse that slot.
    FrameDone {
        slot: u8,
    },
    /// A DevTools protocol call on the page. The engine answers with [`FromHost::CdpResult`] and
    /// the same `id`; the DevTools port is never opened, so only Lynx can reach it.
    Cdp {
        id: u32,
        method: String,
        #[serde(default)]
        params: serde_json::Value,
    },
    Shutdown,
}

/// From the host to Lynx.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum FromHost {
    /// The engine is up and a page is ready to take commands.
    Ready,
    /// A new frame is in `slot` of the shared memory, `width` by `height` device pixels, BGRA.
    Frame {
        slot: u8,
        width: u32,
        height: u32,
    },
    Title {
        title: String,
    },
    Address {
        url: String,
    },
    Loading {
        loading: bool,
        can_go_back: bool,
        can_go_forward: bool,
    },
    LoadFailed {
        url: String,
        message: String,
    },
    /// A DevTools event the page raised, from the few kinds the host passes on (console output,
    /// uncaught errors, requests and where the main frame went).
    CdpEvent {
        method: String,
        params: serde_json::Value,
    },
    /// The page's DevTools agent attached, as it does again when a navigation swaps the page's
    /// process. The domains Lynx turned on are off until it turns them on again.
    DevToolsAttached,
    /// The engine handled something by itself, for the person or the agent to know.
    Notice {
        kind: NoticeKind,
        text: String,
    },
    /// The answer to a [`ToHost::Cdp`]: the call's result, or the error the page's DevTools gave.
    CdpResult {
        id: u32,
        ok: bool,
        result: serde_json::Value,
    },
}

/// Writes one message and flushes, so the other side sees it at once.
pub fn write_message<T: Serialize>(out: &mut impl Write, message: &T) -> io::Result<()> {
    let mut line = serde_json::to_vec(message).map_err(io::Error::other)?;
    line.push(b'\n');
    out.write_all(&line)?;
    out.flush()
}

/// Reads the next message; `None` when the other side closed the connection.
pub fn read_message<T: DeserializeOwned>(input: &mut impl BufRead) -> io::Result<Option<T>> {
    let mut line = String::new();
    if input.read_line(&mut line)? == 0 {
        return Ok(None);
    }
    serde_json::from_str(&line)
        .map(Some)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip<T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug>(message: T) {
        let mut wire = Vec::new();
        write_message(&mut wire, &message).unwrap();
        assert_eq!(wire.iter().filter(|&&byte| byte == b'\n').count(), 1);
        let back: T = read_message(&mut wire.as_slice()).unwrap().unwrap();
        assert_eq!(back, message);
    }

    #[test]
    fn every_command_survives_the_wire() {
        round_trip(ToHost::Navigate {
            url: "https://example.com/?q=ç ã\n".into(),
        });
        round_trip(ToHost::Resize {
            width: 480,
            height: 320,
            scale: 1.5,
        });
        round_trip(ToHost::MouseClick {
            x: 10,
            y: -2,
            button: MouseButton::Right,
            pressed: true,
            clicks: 2,
            modifiers: modifiers::SHIFT | modifiers::LEFT_BUTTON,
        });
        round_trip(ToHost::Key {
            kind: KeyKind::Down,
            windows_code: 13,
            native_code: 36,
            modifiers: 0,
            character: 13,
        });
        round_trip(ToHost::Text {
            text: "ção".into()
        });
        round_trip(ToHost::Reload { ignore_cache: true });
        round_trip(ToHost::Edit {
            command: EditCommand::Paste,
        });
        round_trip(ToHost::FrameDone { slot: 1 });
        round_trip(ToHost::Cdp {
            id: 7,
            method: "Runtime.evaluate".into(),
            params: serde_json::json!({ "expression": "1 + 1" }),
        });
        round_trip(ToHost::Shutdown);
    }

    #[test]
    fn every_event_survives_the_wire() {
        round_trip(FromHost::Ready);
        round_trip(FromHost::Frame {
            slot: 1,
            width: 960,
            height: 640,
        });
        round_trip(FromHost::Title {
            title: "Olá".into(),
        });
        round_trip(FromHost::Loading {
            loading: true,
            can_go_back: false,
            can_go_forward: true,
        });
        round_trip(FromHost::LoadFailed {
            url: "x".into(),
            message: "y".into(),
        });
        round_trip(FromHost::CdpEvent {
            method: "Runtime.consoleAPICalled".into(),
            params: serde_json::json!({ "type": "error" }),
        });
        round_trip(FromHost::DevToolsAttached);
        round_trip(FromHost::Notice {
            kind: NoticeKind::Dialog,
            text: "confirm \"Delete?\" was dismissed".into(),
        });
        round_trip(FromHost::CdpResult {
            id: 7,
            ok: true,
            result: serde_json::json!({ "result": { "value": 2 } }),
        });
    }

    #[test]
    fn editing_shortcuts_are_recognised_with_either_primary_key() {
        let ctrl = modifiers::CONTROL;
        assert_eq!(
            EditCommand::from_shortcut(ctrl, i32::from(b'C')),
            Some(EditCommand::Copy)
        );
        assert_eq!(
            EditCommand::from_shortcut(modifiers::COMMAND, i32::from(b'V')),
            Some(EditCommand::Paste)
        );
        assert_eq!(
            EditCommand::from_shortcut(ctrl, i32::from(b'X')),
            Some(EditCommand::Cut)
        );
        assert_eq!(
            EditCommand::from_shortcut(ctrl, i32::from(b'A')),
            Some(EditCommand::SelectAll)
        );
        assert_eq!(
            EditCommand::from_shortcut(ctrl, i32::from(b'Z')),
            Some(EditCommand::Undo)
        );
        assert_eq!(
            EditCommand::from_shortcut(ctrl | modifiers::SHIFT, i32::from(b'Z')),
            Some(EditCommand::Redo)
        );
        assert_eq!(
            EditCommand::from_shortcut(ctrl, i32::from(b'Y')),
            Some(EditCommand::Redo)
        );
    }

    #[test]
    fn other_keys_and_combinations_are_not_editing_shortcuts() {
        let ctrl = modifiers::CONTROL;
        assert_eq!(EditCommand::from_shortcut(0, i32::from(b'C')), None);
        assert_eq!(
            EditCommand::from_shortcut(modifiers::SHIFT, i32::from(b'C')),
            None
        );
        assert_eq!(
            EditCommand::from_shortcut(ctrl | modifiers::ALT, i32::from(b'C')),
            None
        );
        assert_eq!(
            EditCommand::from_shortcut(ctrl | modifiers::SHIFT, i32::from(b'C')),
            None
        );
        assert_eq!(EditCommand::from_shortcut(ctrl, i32::from(b'Q')), None);
        assert_eq!(EditCommand::from_shortcut(ctrl, 300), None);
    }

    #[test]
    fn the_wire_format_is_stable_and_readable() {
        let mut wire = Vec::new();
        write_message(&mut wire, &ToHost::Reload { ignore_cache: true }).unwrap();
        assert_eq!(wire, b"{\"type\":\"reload\",\"ignoreCache\":true}\n");
    }

    #[test]
    fn a_closed_connection_reads_as_none() {
        let mut empty: &[u8] = b"";
        assert!(read_message::<ToHost>(&mut empty).unwrap().is_none());
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        let mut garbage: &[u8] = b"not json\n";
        let error = read_message::<ToHost>(&mut garbage).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }
}
