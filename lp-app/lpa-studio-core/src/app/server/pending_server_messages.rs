#![cfg_attr(
    not(all(
        any(feature = "browser-worker", feature = "browser-serial-esp32"),
        target_arch = "wasm32"
    )),
    allow(
        dead_code,
        reason = "the pending-message FIFO is only wired into the wasm browser worker and serial adapters; on other targets it is exercised solely by its unit tests"
    )
)]

//! Shared drain-batch -> decode -> FIFO -> pop-one adapter for client transports.
//!
//! Project reads stream as several bounded [`WireServerMessage`] frames per
//! request (16 KiB budget), so a single poll of a link provider can drain a
//! batch containing multiple protocol frames plus interleaved log/status
//! envelopes, while [`ClientIo::receive`](lpa_client::ClientIo::receive)
//! returns exactly one message per call. A naive `receive()` that returns as
//! soon as it decodes the first protocol frame drops every later frame from the
//! same drained batch, which on real hardware produces the "expected project
//! read frame N, got N+1" sequence-gap error.
//!
//! [`PendingServerMessages`] closes that gap: transports drain their provider's
//! buffer, hand the whole batch to [`PendingServerMessages::ingest`], and then
//! call [`PendingServerMessages::pop`] to emit exactly one queued message per
//! `receive()`. Frames decoded from one batch are preserved in FIFO order;
//! non-protocol items are handed back to the transport (via the classifier
//! closure) so logs and status envelopes keep their existing handling even when
//! a protocol frame appeared earlier in the same batch.
//!
//! ## Decode errors mid-batch
//!
//! When a raw item fails to classify/decode, [`ingest`](PendingServerMessages::ingest)
//! **finishes draining the rest of the batch** (so frames already queued *and*
//! any well-formed frames after the corrupt one survive) and then returns the
//! **first** error it saw. The error is never silently lost: the caller still
//! sees it, but only after every recoverable frame from the batch is safely in
//! the FIFO. This is deliberate — the wire stream is effectively dead after
//! corruption, but the frames that already arrived intact are still worth
//! delivering.

use std::collections::VecDeque;

/// Classification of a single raw item drained from a provider batch.
///
/// The transport-specific classifier maps each raw batch item (a browser
/// output envelope, a serial line, ...) into one of these so the shared
/// adapter can queue protocol frames while letting the transport keep handling
/// its own non-protocol items.
pub enum BatchItem<T> {
    /// A decoded protocol message to enqueue for a later `receive()`.
    Protocol(T),
    /// A non-protocol item (log, status, blank line) already handled by the
    /// classifier. Nothing is enqueued.
    Other,
}

/// FIFO of decoded server messages drained from provider batches.
///
/// See the [module docs](self) for the ordering and decode-error contract.
pub struct PendingServerMessages<T> {
    queue: VecDeque<T>,
}

impl<T> Default for PendingServerMessages<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> PendingServerMessages<T> {
    /// Create an empty queue.
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
        }
    }

    /// Drain one provider batch into the FIFO.
    ///
    /// Each raw item is passed to `classify`; [`BatchItem::Protocol`] values are
    /// enqueued in order and [`BatchItem::Other`] values are ignored (the
    /// classifier is expected to have already handled them). If `classify`
    /// returns an error the rest of the batch is still processed and the first
    /// error is returned afterward, so already-decoded frames survive. See the
    /// [module docs](self).
    pub fn ingest<I, E, F>(&mut self, items: I, mut classify: F) -> Result<(), E>
    where
        I: IntoIterator,
        F: FnMut(I::Item) -> Result<BatchItem<T>, E>,
    {
        let mut first_error: Option<E> = None;
        for item in items {
            match classify(item) {
                Ok(BatchItem::Protocol(message)) => self.queue.push_back(message),
                Ok(BatchItem::Other) => {}
                Err(error) => {
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
            }
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    /// Pop the oldest queued message, if any. Transports call this once per
    /// `receive()` before polling the provider again.
    pub fn pop(&mut self) -> Option<T> {
        self.queue.pop_front()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Eq)]
    enum Raw {
        Frame(u32),
        Log(&'static str),
        Corrupt,
    }

    /// Classify a `Raw` item the way a transport would: real frames become
    /// protocol messages, logs are "handled" as `Other`, and `Corrupt` fails to
    /// decode.
    fn classify(item: Raw) -> Result<BatchItem<u32>, &'static str> {
        match item {
            Raw::Frame(value) => Ok(BatchItem::Protocol(value)),
            Raw::Log(_) => Ok(BatchItem::Other),
            Raw::Corrupt => Err("corrupt frame"),
        }
    }

    #[test]
    fn multiple_protocol_frames_in_one_batch_are_all_queued_in_order() {
        let mut pending = PendingServerMessages::new();
        pending
            .ingest([Raw::Frame(1), Raw::Frame(2), Raw::Frame(3)], classify)
            .unwrap();

        assert_eq!(pending.pop(), Some(1));
        assert_eq!(pending.pop(), Some(2));
        assert_eq!(pending.pop(), Some(3));
        assert_eq!(pending.pop(), None);
    }

    #[test]
    fn log_lines_interleaved_with_frames_do_not_drop_frames() {
        let mut pending = PendingServerMessages::new();
        pending
            .ingest(
                [
                    Raw::Log("boot"),
                    Raw::Frame(10),
                    Raw::Log("status"),
                    Raw::Frame(11),
                    Raw::Log("trailing"),
                ],
                classify,
            )
            .unwrap();

        // Both frames survive even though a protocol frame appeared before a
        // later log line, and logs never land in the FIFO.
        assert_eq!(pending.pop(), Some(10));
        assert_eq!(pending.pop(), Some(11));
        assert_eq!(pending.pop(), None);
    }

    #[test]
    fn ordering_is_preserved_across_multiple_ingests_and_pops() {
        let mut pending = PendingServerMessages::new();
        pending
            .ingest([Raw::Frame(1), Raw::Frame(2)], classify)
            .unwrap();
        assert_eq!(pending.pop(), Some(1));

        // A second drained batch queues behind the still-pending frame 2.
        pending
            .ingest([Raw::Frame(3), Raw::Frame(4)], classify)
            .unwrap();
        assert_eq!(pending.pop(), Some(2));
        assert_eq!(pending.pop(), Some(3));
        assert_eq!(pending.pop(), Some(4));
        assert_eq!(pending.pop(), None);
    }

    #[test]
    fn decode_error_mid_batch_keeps_prior_and_later_frames_and_surfaces_error() {
        let mut pending = PendingServerMessages::new();
        let result = pending.ingest([Raw::Frame(1), Raw::Corrupt, Raw::Frame(2)], classify);

        // The error is surfaced, not lost...
        assert_eq!(result, Err("corrupt frame"));
        // ...and every well-formed frame from the batch, before and after the
        // corrupt one, is still queued in order.
        assert_eq!(pending.pop(), Some(1));
        assert_eq!(pending.pop(), Some(2));
        assert_eq!(pending.pop(), None);
    }
}
