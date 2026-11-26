use std::collections::VecDeque;

use super::replay_state::ReplayState;

/// Recent terminal output, so a view that (re)attaches can repaint it.
/// Holds whole chunks and drops the oldest first. The state the dropped output left is kept with the
/// chunks, so a replay that starts after it still sets the modes and attributes it relies on.
pub struct ScrollbackBuffer {
    chunks: VecDeque<String>,
    bytes: usize,
    capacity_bytes: usize,
    /// The terminal state at the start of the first chunk.
    before: ReplayState,
}

impl ScrollbackBuffer {
    pub fn new(capacity_bytes: usize) -> Self {
        Self {
            chunks: VecDeque::new(),
            bytes: 0,
            capacity_bytes,
            before: ReplayState::default(),
        }
    }

    pub fn push(&mut self, chunk: String) {
        self.bytes += chunk.len();
        self.chunks.push_back(chunk);
        while self.bytes > self.capacity_bytes && self.chunks.len() > 1 {
            if let Some(dropped) = self.chunks.pop_front() {
                self.bytes -= dropped.len();
                self.before.feed(dropped.as_bytes());
            }
        }
    }

    pub fn contents(&self) -> String {
        self.chunks.iter().map(String::as_str).collect()
    }

    /// What an attaching view is replayed: the state the dropped output left, then all the output the
    /// buffer still holds. Eviction can cut an escape sequence in two; the text then starts after that
    /// sequence, where the state the preamble sets ends too.
    pub fn replay(&self) -> String {
        let mut state = self.before.clone();
        for (chunk, text) in self.chunks.iter().enumerate() {
            if state.is_ground() {
                return state.preamble() + &self.tail(chunk, 0);
            }
            for (offset, byte) in text.bytes().enumerate() {
                state.feed(&[byte]);
                if state.is_ground() {
                    return state.preamble() + &self.tail(chunk, offset + 1);
                }
            }
        }
        // The whole buffer is one unfinished sequence: the state it set, and no text.
        state.preamble()
    }

    fn tail(&self, chunk: usize, offset: usize) -> String {
        let mut out = String::new();
        for (index, text) in self.chunks.iter().enumerate().skip(chunk) {
            // A sequence ends on an ASCII byte, so `offset` after one is a char boundary.
            out.push_str(if index == chunk {
                &text[offset..]
            } else {
                text
            });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIB: usize = 1024 * 1024;

    fn numbered(count: usize) -> String {
        (0..count).map(|n| format!("{n}\n")).collect()
    }

    #[test]
    fn returns_chunks_in_order() {
        let mut buffer = ScrollbackBuffer::new(100);
        buffer.push("ab".to_owned());
        buffer.push("cd".to_owned());
        assert_eq!(buffer.contents(), "abcd");
    }

    #[test]
    fn drops_the_oldest_chunks_beyond_capacity() {
        let mut buffer = ScrollbackBuffer::new(6);
        for chunk in ["aaa", "bbb", "ccc"] {
            buffer.push(chunk.to_owned());
        }
        assert_eq!(buffer.contents(), "bbbccc");
    }

    #[test]
    fn always_keeps_the_latest_chunk_even_if_it_alone_exceeds_capacity() {
        let mut buffer = ScrollbackBuffer::new(2);
        buffer.push("old".to_owned());
        buffer.push("a-very-long-chunk".to_owned());
        assert_eq!(buffer.contents(), "a-very-long-chunk");
    }

    #[test]
    fn empty_buffer_has_empty_contents() {
        assert_eq!(ScrollbackBuffer::new(10).contents(), "");
    }

    #[test]
    fn an_empty_buffer_replays_nothing() {
        assert_eq!(ScrollbackBuffer::new(MIB).replay(), "");
    }

    #[test]
    fn a_buffer_is_replayed_whole_across_its_chunks() {
        let mut buffer = ScrollbackBuffer::new(MIB);
        for chunk in ["0\n1\n2\n", "3\n4\n5\n", "6\n7\n8\n9\n"] {
            buffer.push(chunk.to_owned());
        }
        assert_eq!(buffer.replay(), numbered(10));
    }

    #[test]
    fn a_buffer_past_two_thousand_lines_is_replayed_whole() {
        let mut buffer = ScrollbackBuffer::new(MIB);
        buffer.push(numbered(3000));
        assert_eq!(buffer.replay(), numbered(3000));
    }

    #[test]
    fn state_dropped_from_the_buffer_is_still_replayed() {
        let mut buffer = ScrollbackBuffer::new(8);
        buffer.push("\x1b[?2004h".to_owned());
        buffer.push("text\n".to_owned());
        assert_eq!(buffer.replay(), "\x1b[?2004htext\n");
    }

    #[test]
    fn a_color_set_in_dropped_output_is_replayed_before_the_text() {
        let mut buffer = ScrollbackBuffer::new(8);
        buffer.push("\x1b[44m".to_owned());
        buffer.push("text\n".to_owned());
        assert_eq!(buffer.replay(), "\x1b[0;44mtext\n");
    }

    #[test]
    fn a_reset_in_dropped_output_forgets_the_state_before_it() {
        let mut buffer = ScrollbackBuffer::new(8);
        buffer.push("\x1b[?2004h\x1bc".to_owned());
        buffer.push("x\n".to_owned());
        assert_eq!(buffer.replay(), "x\n");
    }

    #[test]
    fn eviction_that_cuts_a_sequence_replays_from_after_it() {
        let mut buffer = ScrollbackBuffer::new(8);
        buffer.push("\x1b[?20".to_owned());
        buffer.push("04h\nx\n".to_owned());
        // The first half of the mode set was dropped: the state sets it whole, then the text starts after it.
        assert_eq!(buffer.replay(), "\x1b[?2004h\nx\n");
    }

    #[test]
    fn a_cut_sequence_that_ends_the_buffer_replays_its_state_and_no_text() {
        let mut buffer = ScrollbackBuffer::new(4);
        buffer.push("\x1b[?2".to_owned());
        buffer.push("004h".to_owned());
        assert_eq!(buffer.replay(), "\x1b[?2004h");
    }

    #[test]
    fn a_buffer_inside_one_unfinished_sequence_replays_no_text() {
        let mut buffer = ScrollbackBuffer::new(4);
        buffer.push("\x1b[?2".to_owned());
        buffer.push("004".to_owned());
        assert_eq!(buffer.replay(), "");
    }
}
