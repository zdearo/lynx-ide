//! Text that goes into an agent's input: the one place that decides what may reach a terminal. Line breaks stay, and
//! every other control character goes, so no escape sequence or Enter can hide in a message.

const PASTE_START: &str = "\u{1b}[200~";
const PASTE_END: &str = "\u{1b}[201~";

/// `text` with CRLF and CR read as line breaks, without the paste markers, and without control characters except
/// line breaks and tabs (C0, DEL and the C1 range alike).
pub fn sanitize(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace(PASTE_END, "")
        .replace(PASTE_START, "")
        .chars()
        .filter(|&c| c == '\n' || c == '\t' || !c.is_control())
        .collect()
}

/// `text`, sanitized, between the paste markers: a program that reads it takes its line breaks as text, not as Enter.
pub fn as_paste(text: &str) -> String {
    format!("{PASTE_START}{}{PASTE_END}", sanitize(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_breaks_are_read_as_lf() {
        assert_eq!(sanitize("a\r\nb\rc\nd"), "a\nb\nc\nd");
    }

    #[test]
    fn control_characters_go_but_tabs_and_line_breaks_stay() {
        assert_eq!(
            sanitize("a\tb\nc\u{7}d\u{7f}e\u{9b}f\u{0}g"),
            "a\tb\ncdefg",
            "bell, DEL, the C1 CSI and NUL are dropped"
        );
    }

    #[test]
    fn a_paste_end_cannot_close_the_paste_early() {
        let cleaned = sanitize("uma\u{1b}[201~\rdois");
        assert_eq!(cleaned, "uma\ndois");
        assert!(!cleaned.contains('\u{1b}'));
    }

    #[test]
    fn a_paste_is_the_sanitized_text_between_the_markers() {
        assert_eq!(as_paste("uma\nduas"), "\u{1b}[200~uma\nduas\u{1b}[201~");
        assert_eq!(as_paste("x\u{1b}[201~y"), "\u{1b}[200~xy\u{1b}[201~");
    }
}
