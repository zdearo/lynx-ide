//! Where each session's output goes: a channel per view, carrying raw bytes. An event would
//! put every chunk through JSON (escape characters double in size and the page parses them back);
//! a channel hands the page an `ArrayBuffer`.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use tauri::ipc::{Channel, InvokeResponseBody, IpcResponse};

/// A chunk of output as raw bytes. Sent through a channel it arrives as an `ArrayBuffer`; the
/// type exists because `InvokeResponseBody`, which carries that, has no description for the
/// generated TypeScript contract.
#[derive(specta::Type)]
pub struct OutputBytes(#[specta(type = Vec<u8>)] Vec<u8>);

impl IpcResponse for OutputBytes {
    fn body(self) -> tauri::Result<InvokeResponseBody> {
        Ok(InvokeResponseBody::Raw(self.0))
    }
}

/// One chunk on the wire: its `seq` as 4 little-endian bytes, then the text as UTF-8. The text is
/// a whole number of characters, so the page may hand it to the terminal as it is.
pub fn output_frame(seq: u32, text: &str) -> Vec<u8> {
    let mut frame = Vec::with_capacity(4 + text.len());
    frame.extend_from_slice(&seq.to_le_bytes());
    frame.extend_from_slice(text.as_bytes());
    frame
}

#[derive(Default)]
pub struct OutputChannels {
    by_session: Mutex<HashMap<String, Channel<OutputBytes>>>,
}

impl OutputChannels {
    /// A view that attaches again (a reload) replaces the one before it.
    pub fn attach(&self, id: &str, channel: Channel<OutputBytes>) {
        self.sessions().insert(id.to_owned(), channel);
    }

    pub fn detach(&self, id: &str) {
        self.sessions().remove(id);
    }

    /// Output nobody is attached for is not lost: the session keeps it, and a snapshot has it.
    pub fn send(&self, id: &str, seq: u32, text: &str) {
        // Cloned, so the web view is not called with the lock held.
        let Some(channel) = self.sessions().get(id).cloned() else {
            return;
        };
        if channel.send(OutputBytes(output_frame(seq, text))).is_err() {
            // The page is gone; a new view attaches its own channel.
            self.detach(id);
        }
    }

    // Only the Unix tests that attach a view read this; elsewhere it would be dead code.
    #[cfg(all(test, unix))]
    pub fn is_attached(&self, id: &str) -> bool {
        self.sessions().contains_key(id)
    }

    fn sessions(&self) -> std::sync::MutexGuard<'_, HashMap<String, Channel<OutputBytes>>> {
        self.by_session
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    type Frames = Arc<Mutex<Vec<Vec<u8>>>>;

    fn recording_channel() -> (Channel<OutputBytes>, Frames) {
        let received = Arc::new(Mutex::new(Vec::new()));
        let store = Arc::clone(&received);
        let channel = Channel::new(move |body| {
            if let InvokeResponseBody::Raw(bytes) = body {
                store.lock().expect("lock").push(bytes);
            }
            Ok(())
        });
        (channel, received)
    }

    #[test]
    fn a_frame_is_the_sequence_number_then_the_text() {
        let frame = output_frame(0x0102_0304, "ção\u{1b}[0m");

        assert_eq!(frame[..4], [4, 3, 2, 1]);
        assert_eq!(&frame[4..], "ção\u{1b}[0m".as_bytes());
    }

    #[test]
    fn output_reaches_the_attached_view_as_raw_bytes_in_order() {
        let channels = OutputChannels::default();
        let (channel, received) = recording_channel();
        channels.attach("a", channel);

        channels.send("a", 1, "one");
        channels.send("a", 2, "two");

        let frames = received.lock().expect("lock");
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0], output_frame(1, "one"));
        assert_eq!(frames[1], output_frame(2, "two"));
    }

    #[test]
    fn a_session_with_no_view_attached_drops_nothing_and_fails_nothing() {
        let channels = OutputChannels::default();
        let (channel, received) = recording_channel();
        channels.attach("other", channel);

        channels.send("nobody", 1, "kept by the session instead");

        assert!(received.lock().expect("lock").is_empty());
    }

    #[test]
    fn a_detached_view_receives_nothing_more() {
        let channels = OutputChannels::default();
        let (channel, received) = recording_channel();
        channels.attach("a", channel);
        channels.send("a", 1, "before");

        channels.detach("a");
        channels.send("a", 2, "after");

        assert_eq!(received.lock().expect("lock").len(), 1);
    }

    #[test]
    fn a_view_that_attaches_again_replaces_the_one_before() {
        let channels = OutputChannels::default();
        let (first, first_received) = recording_channel();
        let (second, second_received) = recording_channel();
        channels.attach("a", first);
        channels.attach("a", second);

        channels.send("a", 1, "x");

        assert!(first_received.lock().expect("lock").is_empty());
        assert_eq!(second_received.lock().expect("lock").len(), 1);
    }

    #[test]
    fn a_view_whose_page_is_gone_is_forgotten() {
        let channels = OutputChannels::default();
        let channel = Channel::new(|_| Err(tauri::Error::WebviewNotFound));
        channels.attach("a", channel);

        channels.send("a", 1, "x");

        assert!(channels.sessions().is_empty());
    }
}
