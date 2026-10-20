//! The keys an agent can press, by the names a model naturally writes (`Enter`, `ArrowDown`,
//! `Control+a`), turned into what the engine takes: a Windows key code, the control character the
//! key produces, and the modifier bits.

use lynx_browser_protocol::message::modifiers;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyPress {
    pub windows_code: i32,
    /// The character the key types by itself, or 0 for keys that type nothing.
    pub character: u16,
    pub modifiers: u32,
}

/// Reads `Key` or `Modifier+Modifier+Key`. None for a key Lynx does not know.
pub fn parse(spec: &str) -> Option<KeyPress> {
    let mut parts: Vec<&str> = spec.split('+').map(str::trim).collect();
    // A lone "+" splits into two empty parts; it is the plus key.
    if spec.trim() == "+" {
        parts = vec!["+"];
    }
    let key = parts.pop().filter(|key| !key.is_empty())?;
    let mut bits = 0;
    for modifier in parts {
        bits |= match modifier.to_ascii_lowercase().as_str() {
            "control" | "ctrl" => modifiers::CONTROL,
            "shift" => modifiers::SHIFT,
            "alt" | "option" => modifiers::ALT,
            "meta" | "cmd" | "command" | "super" => modifiers::COMMAND,
            _ => return None,
        };
    }
    let (windows_code, character) = key_code(key)?;
    Some(KeyPress {
        windows_code,
        character,
        modifiers: bits,
    })
}

fn key_code(key: &str) -> Option<(i32, u16)> {
    let named = match key.to_ascii_lowercase().as_str() {
        "enter" | "return" => (13, 13),
        "tab" => (9, 9),
        "escape" | "esc" => (27, 0),
        "backspace" => (8, 8),
        "delete" | "del" => (46, 0),
        "space" | " " => (32, 32),
        "arrowleft" | "left" => (37, 0),
        "arrowup" | "up" => (38, 0),
        "arrowright" | "right" => (39, 0),
        "arrowdown" | "down" => (40, 0),
        "home" => (36, 0),
        "end" => (35, 0),
        "pageup" => (33, 0),
        "pagedown" => (34, 0),
        function if function.starts_with('f') && function.len() > 1 => {
            let number: i32 = function[1..]
                .parse()
                .ok()
                .filter(|n| (1..=12).contains(n))?;
            (111 + number, 0)
        }
        _ => return single_character(key),
    };
    Some(named)
}

/// A letter, digit or the few punctuation marks on the US layout.
fn single_character(key: &str) -> Option<(i32, u16)> {
    let mut chars = key.chars();
    let character = chars.next().filter(|_| chars.next().is_none())?;
    let code = match character {
        'a'..='z' => i32::from(character.to_ascii_uppercase() as u8),
        'A'..='Z' | '0'..='9' => i32::from(character as u8),
        '/' => 191,
        '.' => 190,
        ',' => 188,
        '-' => 189,
        '+' | '=' => 187,
        _ => return None,
    };
    Some((code, u16::try_from(u32::from(character)).ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_keys_have_their_windows_codes() {
        assert_eq!(
            parse("Enter").map(|key| (key.windows_code, key.character)),
            Some((13, 13))
        );
        assert_eq!(parse("ArrowDown").map(|key| key.windows_code), Some(40));
        assert_eq!(parse("escape").map(|key| key.windows_code), Some(27));
        assert_eq!(parse("F5").map(|key| key.windows_code), Some(116));
    }

    #[test]
    fn modifiers_combine_with_a_key() {
        let press = parse("Control+Shift+a").expect("known key");
        assert_eq!(press.modifiers, modifiers::CONTROL | modifiers::SHIFT);
        assert_eq!(press.windows_code, 65);
    }

    #[test]
    fn a_letter_types_itself() {
        let press = parse("b").expect("known key");
        assert_eq!((press.windows_code, press.character), (66, u16::from(b'b')));
    }

    #[test]
    fn unknown_keys_and_modifiers_are_refused() {
        assert_eq!(parse("Hyper+a"), None);
        assert_eq!(parse("Banana"), None);
        assert_eq!(parse("F13"), None);
        assert_eq!(parse(""), None);
        assert_eq!(parse("Control+"), None);
    }
}
