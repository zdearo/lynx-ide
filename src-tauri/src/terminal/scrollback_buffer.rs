use std::collections::VecDeque;

/// Recent terminal output, so a view that (re)attaches can repaint it.
/// Holds whole chunks and drops the oldest first, so the start may land
/// mid-line or mid-escape-sequence; that is acceptable for a repaint.
pub struct ScrollbackBuffer {
    chunks: VecDeque<String>,
    bytes: usize,
    capacity_bytes: usize,
}

impl ScrollbackBuffer {
    pub fn new(capacity_bytes: usize) -> Self {
        Self {
            chunks: VecDeque::new(),
            bytes: 0,
            capacity_bytes,
        }
    }

    pub fn push(&mut self, chunk: String) {
        self.bytes += chunk.len();
        self.chunks.push_back(chunk);
        while self.bytes > self.capacity_bytes && self.chunks.len() > 1 {
            if let Some(dropped) = self.chunks.pop_front() {
                self.bytes -= dropped.len();
            }
        }
    }

    pub fn contents(&self) -> String {
        self.chunks.iter().map(String::as_str).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
