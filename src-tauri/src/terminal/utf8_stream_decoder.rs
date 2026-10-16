/// Turns raw PTY bytes into text without splitting a multi-byte character that
/// straddles two reads. Invalid bytes become U+FFFD, like a terminal would draw them.
#[derive(Default)]
pub struct Utf8StreamDecoder {
    /// An incomplete character from the previous read: at most 3 bytes.
    pending: Vec<u8>,
}

impl Utf8StreamDecoder {
    pub fn push(&mut self, bytes: &[u8]) -> String {
        self.pending.extend_from_slice(bytes);

        let mut text = String::new();
        loop {
            match std::str::from_utf8(&self.pending) {
                Ok(valid) => {
                    text.push_str(valid);
                    self.pending.clear();
                    return text;
                }
                Err(error) => {
                    let valid_up_to = error.valid_up_to();
                    text.push_str(&String::from_utf8_lossy(&self.pending[..valid_up_to]));
                    match error.error_len() {
                        Some(invalid_len) => {
                            text.push(char::REPLACEMENT_CHARACTER);
                            self.pending.drain(..valid_up_to + invalid_len);
                        }
                        None => {
                            self.pending.drain(..valid_up_to);
                            return text;
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passes_ascii_through() {
        assert_eq!(Utf8StreamDecoder::default().push(b"hello"), "hello");
    }

    #[test]
    fn holds_back_a_character_split_across_reads() {
        let bytes = "é→😀".as_bytes();
        let mut decoder = Utf8StreamDecoder::default();
        let mut text = String::new();
        // One byte at a time is the worst case.
        for byte in bytes {
            text.push_str(&decoder.push(&[*byte]));
        }
        assert_eq!(text, "é→😀");
    }

    #[test]
    fn emits_nothing_until_the_character_is_complete() {
        let mut decoder = Utf8StreamDecoder::default();
        assert_eq!(decoder.push(&[0xF0, 0x9F]), "");
        assert_eq!(decoder.push(&[0x98, 0x80]), "😀");
    }

    #[test]
    fn replaces_invalid_bytes_and_keeps_going() {
        let mut decoder = Utf8StreamDecoder::default();
        assert_eq!(decoder.push(b"a\xFFb\xC0c"), "a\u{FFFD}b\u{FFFD}c");
    }

    #[test]
    fn invalid_byte_before_a_split_character_does_not_lose_it() {
        let mut decoder = Utf8StreamDecoder::default();
        assert_eq!(decoder.push(&[0xFF, 0xE2, 0x82]), "\u{FFFD}");
        assert_eq!(decoder.push(&[0xAC]), "€");
    }
}
