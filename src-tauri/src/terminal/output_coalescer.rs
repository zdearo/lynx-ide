//! Joins the chunks a busy session reads into fewer, bigger ones, so the webview is sent
//! hundreds of events a second instead of hundreds of thousands. A session that was quiet is
//! never delayed: its first chunk goes out at once.

use std::sync::mpsc::{Receiver, RecvTimeoutError, TryRecvError};
use std::time::{Duration, Instant};

/// The shortest time between two emissions of one session that is in a burst.
const MIN_GAP: Duration = Duration::from_millis(4);
/// Emissions in a row, each within two gaps of the last, before a session counts as in a burst.
/// A typed key and the short answer to a command stay under it, so they are never held back.
const BURST_AFTER: u32 = 2;
/// A batch this big goes out at once, whatever the gap: one huge write would stall the page.
const MAX_BATCH_BYTES: usize = 64 * 1024;

/// Calls `emit` with the text read so far, in order, until the sender is gone.
pub fn coalesce(chunks: &Receiver<String>, mut emit: impl FnMut(String)) {
    let mut last_emit: Option<Instant> = None;
    let mut in_a_row = 0;
    while let Ok(mut batch) = chunks.recv() {
        in_a_row = match last_emit {
            Some(at) if at.elapsed() < MIN_GAP * 2 => in_a_row + 1,
            _ => 0,
        };
        let due = last_emit
            .filter(|_| in_a_row >= BURST_AFTER)
            .map(|at| at + MIN_GAP);
        let mut closed = false;
        while batch.len() < MAX_BATCH_BYTES {
            let wait = due.map_or(Duration::ZERO, |due| {
                due.saturating_duration_since(Instant::now())
            });
            let next = if wait.is_zero() {
                chunks.try_recv().map_err(|error| match error {
                    TryRecvError::Empty => RecvTimeoutError::Timeout,
                    TryRecvError::Disconnected => RecvTimeoutError::Disconnected,
                })
            } else {
                chunks.recv_timeout(wait)
            };
            match next {
                Ok(text) => batch.push_str(&text),
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => {
                    closed = true;
                    break;
                }
            }
        }
        emit(batch);
        last_emit = Some(Instant::now());
        if closed {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::channel;

    use super::*;

    /// A guard against a hang, not a timing claim: Windows timers are too coarse for those.
    const WAIT: Duration = Duration::from_secs(5);

    #[test]
    fn the_first_chunk_of_a_quiet_session_goes_out_while_the_session_is_still_open() {
        let (send, receive) = channel();
        let (out, emitted) = channel();
        let worker = std::thread::spawn(move || {
            coalesce(&receive, |text| out.send(text).expect("out"));
        });
        send.send("a".to_owned()).expect("send");
        // The sender is still open: the chunk must not wait for the stream to end.
        assert_eq!(emitted.recv_timeout(WAIT).expect("held back"), "a");
        drop(send);
        worker.join().expect("join");
    }

    #[test]
    fn a_burst_becomes_few_batches_and_keeps_its_order() {
        let (send, receive) = channel();
        for number in 0..2000 {
            send.send(format!("{number},")).expect("send");
        }
        drop(send);
        let mut batches = Vec::new();
        coalesce(&receive, |text| batches.push(text));
        let expected: String = (0..2000).map(|number| format!("{number},")).collect();
        assert_eq!(batches.concat(), expected);
        assert!(batches.len() < 20, "{} batches", batches.len());
    }

    #[test]
    fn a_short_answer_right_after_an_echo_is_not_held_back() {
        let (send, receive) = channel();
        let (out, emitted) = channel();
        let worker = std::thread::spawn(move || {
            coalesce(&receive, |text| out.send(text).expect("out"));
        });
        send.send("echo".to_owned()).expect("send");
        assert_eq!(emitted.recv_timeout(WAIT).expect("echo held back"), "echo");
        send.send("answer".to_owned()).expect("send");
        // Still open: an answer held back for the gap would wait for the stream to end.
        assert_eq!(
            emitted.recv_timeout(WAIT).expect("answer held back"),
            "answer"
        );
        drop(send);
        worker.join().expect("join");
    }

    #[test]
    fn what_is_waiting_when_the_sender_leaves_is_still_emitted() {
        let (send, receive) = channel();
        send.send("last".to_owned()).expect("send");
        drop(send);
        let mut batches = Vec::new();
        coalesce(&receive, |text| batches.push(text));
        assert_eq!(batches, ["last"]);
    }

    #[test]
    fn a_huge_batch_is_cut_instead_of_growing_without_end() {
        let (send, receive) = channel();
        for _ in 0..8 {
            send.send("x".repeat(100 * 1024)).expect("send");
        }
        drop(send);
        let mut batches = Vec::new();
        coalesce(&receive, |text| batches.push(text.len()));
        assert!(
            batches
                .iter()
                .all(|&len| len < MAX_BATCH_BYTES + 100 * 1024)
        );
        assert_eq!(batches.iter().sum::<usize>(), 800 * 1024);
    }
}
