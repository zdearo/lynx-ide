//! Reads window titles (OSC 0 and OSC 2) out of terminal output. A sequence may be split
//! across two chunks, so the scanner keeps its place between calls.

/// Longer titles are cut here; the status rules only look at the start of a title anyway.
const MAX_TITLE_BYTES: usize = 1024;

#[derive(Default)]
pub struct TitleScanner {
    state: ScanState,
}

#[derive(Default)]
enum ScanState {
    #[default]
    Ground,
    /// An ESC was seen; the next character decides what kind of sequence this is.
    Escape,
    /// Inside `ESC ]`, collecting the numeric code before the `;`.
    OscCode { code: String },
    /// Inside a title, which ends at BEL or at `ESC \`.
    Title { text: String, escape: bool },
    /// Inside an OSC whose text is not a title, skipped up to its end.
    Ignored { escape: bool },
}

impl TitleScanner {
    /// Feeds the next chunk of output and returns the titles that finished in it, in order.
    pub fn push(&mut self, text: &str) -> Vec<String> {
        let mut titles = Vec::new();
        for ch in text.chars() {
            let state = std::mem::take(&mut self.state);
            self.state = step(state, ch, &mut titles);
        }
        titles
    }
}

fn step(state: ScanState, ch: char, titles: &mut Vec<String>) -> ScanState {
    match state {
        ScanState::Ground => {
            if ch == '\x1b' {
                ScanState::Escape
            } else {
                ScanState::Ground
            }
        }
        ScanState::Escape => match ch {
            ']' => ScanState::OscCode {
                code: String::new(),
            },
            '\x1b' => ScanState::Escape,
            _ => ScanState::Ground,
        },
        ScanState::OscCode { mut code } => match ch {
            ';' if code == "0" || code == "2" => ScanState::Title {
                text: String::new(),
                escape: false,
            },
            ';' => ScanState::Ignored { escape: false },
            '\x07' => ScanState::Ground,
            '\x1b' => ScanState::Ignored { escape: true },
            digit if digit.is_ascii_digit() && code.len() < 4 => {
                code.push(digit);
                ScanState::OscCode { code }
            }
            _ => ScanState::Ignored { escape: false },
        },
        ScanState::Title { mut text, escape } => {
            if escape {
                // `ESC \` ends the title; any other escape abandons it.
                if ch == '\\' {
                    titles.push(text);
                }
                ScanState::Ground
            } else if ch == '\x07' {
                titles.push(text);
                ScanState::Ground
            } else if ch == '\x1b' {
                ScanState::Title { text, escape: true }
            } else {
                if text.len() < MAX_TITLE_BYTES {
                    text.push(ch);
                }
                ScanState::Title {
                    text,
                    escape: false,
                }
            }
        }
        ScanState::Ignored { escape } => {
            if ch == '\x07' || (escape && ch == '\\') {
                ScanState::Ground
            } else if ch == '\x1b' {
                ScanState::Ignored { escape: true }
            } else if escape {
                ScanState::Ground
            } else {
                ScanState::Ignored { escape: false }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_an_osc_2_title_ended_by_bel() {
        let mut scanner = TitleScanner::default();
        assert_eq!(scanner.push("\x1b]2;hello\x07"), vec!["hello"]);
    }

    #[test]
    fn reads_an_osc_0_title_ended_by_st() {
        let mut scanner = TitleScanner::default();
        assert_eq!(scanner.push("\x1b]0;Claude\x1b\\"), vec!["Claude"]);
    }

    #[test]
    fn a_title_split_across_chunks_is_joined() {
        let mut scanner = TitleScanner::default();
        assert!(scanner.push("\x1b]2;hel").is_empty());
        assert_eq!(scanner.push("lo\x07"), vec!["hello"]);
    }

    #[test]
    fn an_escape_split_from_its_bracket_is_still_recognised() {
        let mut scanner = TitleScanner::default();
        assert!(scanner.push("\x1b").is_empty());
        assert_eq!(scanner.push("]2;split\x07"), vec!["split"]);
    }

    #[test]
    fn an_st_split_across_chunks_still_ends_the_title() {
        let mut scanner = TitleScanner::default();
        assert!(scanner.push("\x1b]2;end\x1b").is_empty());
        assert_eq!(scanner.push("\\"), vec!["end"]);
    }

    #[test]
    fn other_osc_sequences_are_skipped() {
        let mut scanner = TitleScanner::default();
        assert!(scanner.push("\x1b]7;file://host/tmp\x07").is_empty());
        assert!(scanner.push("\x1b]8;;https://example.com\x1b\\").is_empty());
    }

    #[test]
    fn plain_text_and_other_escapes_yield_no_titles() {
        let mut scanner = TitleScanner::default();
        assert!(
            scanner
                .push("hello \x1b[31mred\x1b[0m world\r\n")
                .is_empty()
        );
    }

    #[test]
    fn several_titles_in_one_chunk_come_out_in_order() {
        let mut scanner = TitleScanner::default();
        assert_eq!(
            scanner.push("\x1b]2;one\x07text\x1b]2;two\x07"),
            vec!["one", "two"]
        );
    }

    #[test]
    fn a_title_keeps_non_ascii_text() {
        let mut scanner = TitleScanner::default();
        assert_eq!(scanner.push("\x1b]2;⠋ Claude\x07"), vec!["⠋ Claude"]);
    }

    #[test]
    fn an_overlong_title_is_cut_but_still_ends_cleanly() {
        let mut scanner = TitleScanner::default();
        let long = "a".repeat(MAX_TITLE_BYTES * 2);
        let titles = scanner.push(&format!("\x1b]2;{long}\x07next"));
        assert_eq!(titles.len(), 1);
        assert_eq!(titles[0].len(), MAX_TITLE_BYTES);
    }
}
