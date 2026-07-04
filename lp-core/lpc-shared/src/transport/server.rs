//! Server-side transport trait
//!
//! Defines the interface for server-side transport implementations.
//! Messages are consumed (moved) on send, and receive is non-blocking.
//!
//! The transport handles serialization/deserialization internally.
//! Project reads use [`ProjectReadStreamSink`] so every transport shares the
//! same bounded batching policy and the same envelope sequencing (`seq`/`fin`).

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use lpc_wire::{
    PROJECT_READ_FRAME_MAX_BYTES, ProjectReadEvent, TransportError, WireServerMessage,
    messages::ClientMessage,
};

/// Sink for semantic project-read events.
///
/// The engine emits typed events into this trait. Transport code decides how to
/// batch those events into bounded wire messages.
#[allow(
    async_fn_in_trait,
    reason = "server transport traits already use async fn"
)]
pub trait ProjectReadEventSink {
    type Error;

    async fn send_project_read_event(&mut self, event: ProjectReadEvent)
    -> Result<(), Self::Error>;
}

/// Trait for server-side transport implementations
///
/// This trait provides a simple polling-based interface for sending and receiving
/// messages. Messages are consumed (moved) on send, and receive is non-blocking
/// (returns `None` if no message is available).
///
/// The transport handles serialization/deserialization internally.
///
/// Separate from `ClientTransport` for clarity, even though the interface is
/// similar. This allows for different implementations or future extensions
/// specific to server-side use cases.
///
/// # Examples
///
/// ```rust,no_run
/// use lpc_shared::transport::ServerTransport;
/// use lpc_wire::{ClientMessage, TransportError};
/// use lpc_wire::WireServerMessage;
///
/// struct MyTransport;
///
/// impl ServerTransport for MyTransport {
///     async fn send(&mut self, msg: WireServerMessage) -> Result<(), TransportError> {
///         // Send message (transport handles serialization)
///         let _ = msg;
///         Ok(())
///     }
///
///     async fn receive(&mut self) -> Result<Option<ClientMessage>, TransportError> {
///         // Receive message (transport handles deserialization)
///         Ok(None)
///     }
///
///     async fn receive_all(&mut self) -> Result<Vec<ClientMessage>, TransportError> {
///         Ok(Vec::new())
///     }
///
///     async fn close(&mut self) -> Result<(), TransportError> {
///         // Close the transport connection
///         Ok(())
///     }
/// }
/// ```
#[allow(async_fn_in_trait, reason = "trait async fn stable in Rust 1.75+")]
pub trait ServerTransport {
    /// Send a server message (consumes the message).
    ///
    /// A successful return means the message has been accepted by the
    /// underlying transport write path. Implementations must not report success
    /// for a best-effort handoff that can still drop the message later without
    /// surfacing an error to this future.
    async fn send(&mut self, msg: WireServerMessage) -> Result<(), TransportError>;

    /// Receive a client message (non-blocking). Returns `Ok(None)` if no message is available.
    async fn receive(&mut self) -> Result<Option<ClientMessage>, TransportError>;

    /// Receive all available client messages (non-blocking)
    async fn receive_all(&mut self) -> Result<Vec<ClientMessage>, TransportError>;

    /// Close the transport connection
    async fn close(&mut self) -> Result<(), TransportError>;
}

/// Shared bounded project-read stream sink used by all server transports.
///
/// It batches [`ProjectReadEvent`] values to a byte budget and sequences the
/// batches through the envelope: each emitted message carries `seq` (the
/// monotonic frame number, from an internal counter) and `fin` (finality).
/// Project reads are its first user, but the batching + envelope-sequencing
/// policy is generic over the event body.
///
/// # Finality (`fin`) stamping
///
/// - A budget-triggered flush (the current batch is full and another event
///   needs a fresh frame) sends `fin = false` — more frames follow.
/// - [`finish`](Self::finish) sends the trailing partial batch with
///   `fin = true`. If nothing is pending at `finish` (the previous flush drained
///   everything), it still emits an **empty** `ProjectRead { events: [] }` frame
///   with `fin = true`, so a stream always terminates with an explicit final
///   message the client can key finality off — never a silent stall.
/// - [`send_terminal_error`](Self::send_terminal_error) sends `fin = true`.
///
/// # Measurement and batching
///
/// The sink measures frames with the **wire serializer** (`ser-write-json`,
/// the same one the ESP32 firmware writes with) rather than `serde_json`. The
/// two diverge on float formatting, so budgeting with `serde_json` can
/// under-count the real on-wire bytes; measuring with the wire serializer keeps
/// the 16 KiB budget honest.
///
/// Batching is O(events), not O(frame): each event is measured once on push and
/// its encoded length accumulated. The full frame is only serialized by the
/// transport at send time. A non-final `ProjectRead` body encodes as
/// `{"id":..,"seq":..,"fin":false,"msg":{"projectRead":{"events":[e0,e1,..]}}}`,
/// so the total encoded length is
/// `empty_frame_len(seq) + sum(event_len) + (n - 1)` commas — the empty frame
/// already contains the two `[]` brackets, and each additional event adds one
/// comma separator. The empty-frame envelope is measured with `fin = false` (the
/// worst case: both `seq` and `fin` present), so the budget is never optimistic.
pub struct ProjectReadStreamSink<'a, T> {
    transport: &'a mut T,
    id: u64,
    sequence: u32,
    pending_events: Vec<ProjectReadEvent>,
    /// Cumulative encoded length of `pending_events` as they appear inside the
    /// `events` array: `sum(event_len)` plus one comma between adjacent events.
    /// Zero when `pending_events` is empty.
    pending_events_len: usize,
    /// Encoded length of an empty non-final frame at the current `sequence`
    /// (envelope + `[]`). Recomputed whenever `sequence` changes.
    empty_frame_len: usize,
    max_bytes: usize,
}

impl<'a, T> ProjectReadStreamSink<'a, T>
where
    T: ServerTransport,
{
    pub fn new(transport: &'a mut T, id: u64) -> Self {
        Self::with_max_bytes(transport, id, PROJECT_READ_FRAME_MAX_BYTES)
    }

    pub fn with_max_bytes(transport: &'a mut T, id: u64, max_bytes: usize) -> Self {
        let empty_frame_len = Self::measure_empty_frame_len(id, 0);
        Self {
            transport,
            id,
            sequence: 0,
            pending_events: Vec::new(),
            pending_events_len: 0,
            empty_frame_len,
            max_bytes,
        }
    }

    /// Emit the trailing batch as the final (`fin = true`) frame.
    ///
    /// If the batch is empty (a previous budget flush drained everything), an
    /// empty final frame is still sent so finality is always signaled on the
    /// wire.
    pub async fn finish(&mut self) -> Result<(), TransportError> {
        self.flush(true).await
    }

    /// Best-effort emit a terminal, final [`ProjectReadEvent::Error`] frame.
    ///
    /// Called when the event stream failed with a *signalable* error (an event
    /// too large for an empty frame, or another serialization/budget failure).
    /// Any partially batched events are discarded — the stream is already
    /// broken — and a standalone `Error` frame is sent at the current sequence
    /// with `fin = true` so the client sees a terminal failure for this request
    /// id instead of a silent stall. A transport-write failure while emitting
    /// this frame is returned to the caller but is expected to be logged and
    /// ignored.
    pub async fn send_terminal_error(&mut self, message: String) -> Result<(), TransportError> {
        // Drop any pending batch; those events could not be delivered anyway.
        self.pending_events.clear();
        self.pending_events_len = 0;
        let sequence = self.sequence;
        self.transport
            .send(WireServerMessage::stream_frame(
                self.id,
                sequence,
                true,
                lpc_wire::server::ServerMsgBody::ProjectRead {
                    events: vec![ProjectReadEvent::Error { message }],
                },
            ))
            .await?;
        self.advance_sequence();
        Ok(())
    }

    async fn push_event(&mut self, event: ProjectReadEvent) -> Result<(), TransportError> {
        // Measure this event once with the wire serializer. Its contribution to
        // the encoded frame is its own length plus one comma separator when it
        // follows another event in the array.
        let event_len = lpc_wire::ser_write_json_len(&event);
        let separator = usize::from(!self.pending_events.is_empty());
        let candidate_events_len = self
            .pending_events_len
            .saturating_add(event_len)
            .saturating_add(separator);

        if self.empty_frame_len.saturating_add(candidate_events_len) <= self.max_bytes {
            self.pending_events.push(event);
            self.pending_events_len = candidate_events_len;
            return Ok(());
        }

        // The event does not fit alongside the current batch. Flush the current
        // batch as a non-final frame, then this event becomes the sole occupant
        // of a fresh frame.
        if !self.pending_events.is_empty() {
            self.flush(false).await?;
        }

        // Empty frame length may have changed with the new sequence.
        if self.empty_frame_len.saturating_add(event_len) > self.max_bytes {
            return Err(TransportError::Serialization(format!(
                "project-read event exceeded frame budget of {} bytes",
                self.max_bytes
            )));
        }

        self.pending_events.push(event);
        self.pending_events_len = event_len;
        Ok(())
    }

    /// Send the pending batch as one frame stamped `fin`.
    ///
    /// A non-final flush (`fin == false`) with an empty batch is a no-op. A final
    /// flush (`fin == true`) always sends, emitting an empty
    /// `ProjectRead { events: [] }` frame if the batch is empty so finality is
    /// signaled even when the previous flush drained everything.
    async fn flush(&mut self, fin: bool) -> Result<(), TransportError> {
        if self.pending_events.is_empty() && !fin {
            return Ok(());
        }

        let sequence = self.sequence;
        // The frame owns its events (one clone into the message), but we keep
        // `pending_events` intact so a send failure leaves the batch pending
        // without a second up-front clone-to-restore.
        self.transport
            .send(WireServerMessage::stream_frame(
                self.id,
                sequence,
                fin,
                lpc_wire::server::ServerMsgBody::ProjectRead {
                    events: self.pending_events.clone(),
                },
            ))
            .await?;
        // Confirmed send: clear the batch and advance.
        self.pending_events.clear();
        self.pending_events_len = 0;
        self.advance_sequence();
        Ok(())
    }

    /// Advance to the next frame sequence and refresh the cached empty-frame
    /// length, which depends on the sequence number's digit count.
    fn advance_sequence(&mut self) {
        self.sequence = self.sequence.saturating_add(1);
        self.empty_frame_len = Self::measure_empty_frame_len(self.id, self.sequence);
    }

    /// Encoded length of an empty non-final frame (`events: []`, `fin = false`)
    /// at `sequence`, measured with the wire serializer. Used as the fixed
    /// envelope cost that per-event lengths are added to. Non-final is the worst
    /// case (`fin:false` is present), so budgeting against it never overshoots.
    fn measure_empty_frame_len(id: u64, sequence: u32) -> usize {
        lpc_wire::ser_write_json_len(&WireServerMessage::stream_frame(
            id,
            sequence,
            false,
            lpc_wire::server::ServerMsgBody::ProjectRead { events: Vec::new() },
        ))
    }
}

impl<T> ProjectReadEventSink for ProjectReadStreamSink<'_, T>
where
    T: ServerTransport,
{
    type Error = TransportError;

    async fn send_project_read_event(
        &mut self,
        event: ProjectReadEvent,
    ) -> Result<(), Self::Error> {
        self.push_event(event).await
    }
}

/// Whether a project-read sink failure can be signaled to the client.
///
/// Serialization/budget failures (an event too large for an empty frame, other
/// encoding failures) are *signalable*: the connection is still alive, so the
/// server can send a terminal [`ProjectReadEvent::Error`] frame for the request
/// id. Connection/other transport-write failures cannot be signaled — the write
/// path is the very thing that failed — and must propagate as today.
#[must_use]
pub fn transport_error_is_signalable(error: &TransportError) -> bool {
    matches!(error, TransportError::Serialization(_))
}

#[cfg(test)]
mod tests {
    use alloc::boxed::Box;
    use alloc::vec::Vec;
    use core::future::Future;
    use core::pin::Pin;
    use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

    use lpc_model::Revision;
    use lpc_wire::{ClientMessage, ProjectReadEvent};

    use super::*;

    #[test]
    fn frame_sink_keeps_exact_budget_event() {
        let event = ProjectReadEvent::Begin {
            revision: Revision::new(7),
        };
        let max_bytes = encoded_project_read_frame_len(9, 0, &[event.clone()]);
        let mut transport = CollectingTransport::default();

        block_on(async {
            let mut sink = ProjectReadStreamSink::with_max_bytes(&mut transport, 9, max_bytes);
            sink.send_project_read_event(event).await.unwrap();
            sink.finish().await.unwrap();
        });

        assert_eq!(transport.sent.len(), 1);
        assert_frame(&transport.sent[0], 9, 0, 1);
    }

    #[test]
    fn frame_sink_splits_before_crossing_budget() {
        let begin = ProjectReadEvent::Begin {
            revision: Revision::new(7),
        };
        let end = ProjectReadEvent::End {
            revision: Revision::new(7),
        };
        // Budget = one event in a seq-1 envelope. seq 0 skips the `"seq"` key so
        // its envelope is smaller and comfortably fits; seq 1+ is the worst case
        // these tests exercise, so deriving the budget there keeps it stable
        // across every frame the sink emits.
        let max_bytes = encoded_project_read_frame_len(9, 1, &[begin.clone()]);
        let mut transport = CollectingTransport::default();

        block_on(async {
            let mut sink = ProjectReadStreamSink::with_max_bytes(&mut transport, 9, max_bytes);
            sink.send_project_read_event(begin).await.unwrap();
            sink.send_project_read_event(end).await.unwrap();
            sink.finish().await.unwrap();
        });

        assert_eq!(transport.sent.len(), 2);
        assert_frame(&transport.sent[0], 9, 0, 1);
        assert_frame(&transport.sent[1], 9, 1, 1);
        // A budget-triggered flush is non-final; the trailing `finish` frame is
        // final.
        assert_fin(&transport.sent[0], false);
        assert_fin(&transport.sent[1], true);
    }

    #[test]
    fn frame_sink_finish_flushes_partial_final_frame() {
        let event = ProjectReadEvent::Begin {
            revision: Revision::new(7),
        };
        let max_bytes = encoded_project_read_frame_len(9, 0, &[event.clone()]) * 2;
        let mut transport = CollectingTransport::default();

        block_on(async {
            let mut sink = ProjectReadStreamSink::with_max_bytes(&mut transport, 9, max_bytes);
            sink.send_project_read_event(event).await.unwrap();
            assert!(sink.transport.sent.is_empty());
            sink.finish().await.unwrap();
        });

        assert_eq!(transport.sent.len(), 1);
        assert_frame(&transport.sent[0], 9, 0, 1);
        assert_fin(&transport.sent[0], true);
    }

    #[test]
    fn frame_sink_finish_emits_empty_final_frame_when_nothing_pending() {
        // The rare empty-final case: `finish` is called with nothing pending
        // (here, a read that produced no events at all — the previous flush, if
        // any, drained everything). `finish` still emits an explicit empty final
        // frame so finality is always signaled on the wire — never a silent
        // stall. (New contract in M6/P1: the envelope owns finality.)
        let mut transport = CollectingTransport::default();

        block_on(async {
            let mut sink = ProjectReadStreamSink::with_max_bytes(&mut transport, 9, usize::MAX);
            sink.finish().await.unwrap();
        });

        assert_eq!(transport.sent.len(), 1);
        assert_frame(&transport.sent[0], 9, 0, 0);
        assert_fin(&transport.sent[0], true);
    }

    #[test]
    fn frame_sink_rejects_oversized_single_event() {
        let event = ProjectReadEvent::Begin {
            revision: Revision::new(7),
        };
        let max_bytes = encoded_project_read_frame_len(9, 0, &[event.clone()]).saturating_sub(1);
        let mut transport = CollectingTransport::default();

        let error = block_on(async {
            let mut sink = ProjectReadStreamSink::with_max_bytes(&mut transport, 9, max_bytes);
            sink.send_project_read_event(event).await.unwrap_err()
        });

        assert!(matches!(error, TransportError::Serialization(_)));
        assert!(transport.sent.is_empty());
    }

    #[test]
    fn oversized_event_yields_terminal_error_frame_to_client() {
        // An event that cannot fit an empty frame fails the stream with a
        // signalable serialization error. The server must be able to deliver a
        // terminal `Error` frame to the client for the same request id so the
        // client sees a failure instead of a silent stall / watchdog timeout.
        let event = ProjectReadEvent::Begin {
            revision: Revision::new(7),
        };
        // One byte below the encoded size makes even this single event oversized.
        let max_bytes = encoded_project_read_frame_len(9, 0, &[event.clone()]).saturating_sub(1);
        let mut transport = CollectingTransport::default();

        block_on(async {
            let mut sink = ProjectReadStreamSink::with_max_bytes(&mut transport, 9, max_bytes);
            // The oversized event cannot fit an empty frame: signalable failure.
            let error = sink.send_project_read_event(event).await.unwrap_err();
            assert!(transport_error_is_signalable(&error));
            // Best-effort terminal error frame for this request id.
            sink.send_terminal_error(alloc::format!("{error}"))
                .await
                .unwrap();
        });

        // The only thing the client receives is the terminal error frame, and it
        // is stamped final (`fin = true`).
        assert_eq!(transport.sent.len(), 1);
        assert_eq!(transport.sent[0].id, 9);
        assert_fin(&transport.sent[0], true);
        let lpc_wire::server::ServerMsgBody::ProjectRead { events } = &transport.sent[0].msg else {
            panic!("expected project-read events body");
        };
        assert_eq!(transport.sent[0].seq, 0);
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], ProjectReadEvent::Error { .. }));
    }

    #[test]
    fn connection_lost_is_not_signalable() {
        assert!(!transport_error_is_signalable(
            &TransportError::ConnectionLost
        ));
        assert!(!transport_error_is_signalable(&TransportError::Other(
            "usb dead".into()
        )));
        assert!(transport_error_is_signalable(
            &TransportError::Serialization("too big".into())
        ));
    }

    #[test]
    fn frame_sink_does_not_advance_sequence_after_send_error() {
        let begin = ProjectReadEvent::Begin {
            revision: Revision::new(7),
        };
        let end = ProjectReadEvent::End {
            revision: Revision::new(7),
        };
        // seq-1 worst-case budget: stable across the seq 0/1 frames emitted here.
        let max_bytes = encoded_project_read_frame_len(9, 1, &[begin.clone()]);
        let mut transport = FailingOnceTransport::default();

        block_on(async {
            let mut sink = ProjectReadStreamSink::with_max_bytes(&mut transport, 9, max_bytes);
            sink.send_project_read_event(begin).await.unwrap();
            sink.send_project_read_event(end.clone()).await.unwrap_err();
            sink.send_project_read_event(end).await.unwrap();
            sink.finish().await.unwrap();
        });

        assert_eq!(transport.inner.sent.len(), 2);
        assert_frame(&transport.inner.sent[0], 9, 0, 1);
        assert_frame(&transport.inner.sent[1], 9, 1, 1);
    }

    #[test]
    fn frame_sink_stops_at_failed_sequence_without_emitting_later_frames() {
        let event = ProjectReadEvent::Begin {
            revision: Revision::new(7),
        };
        // seq-1 worst-case budget so every seq 0..3 frame fits identically.
        let max_bytes = encoded_project_read_frame_len(9, 1, &[event.clone()]);
        let mut transport = FailingSequenceTransport::new(3);

        block_on(async {
            let mut sink = ProjectReadStreamSink::with_max_bytes(&mut transport, 9, max_bytes);
            sink.send_project_read_event(event.clone()).await.unwrap();
            sink.send_project_read_event(event.clone()).await.unwrap();
            sink.send_project_read_event(event.clone()).await.unwrap();
            sink.send_project_read_event(event.clone()).await.unwrap();
            sink.send_project_read_event(event).await.unwrap_err();
        });

        assert_eq!(transport.inner.sent.len(), 3);
        assert_frame(&transport.inner.sent[0], 9, 0, 1);
        assert_frame(&transport.inner.sent[1], 9, 1, 1);
        assert_frame(&transport.inner.sent[2], 9, 2, 1);
    }

    #[derive(Default)]
    struct CollectingTransport {
        sent: Vec<WireServerMessage>,
    }

    impl ServerTransport for CollectingTransport {
        async fn send(&mut self, msg: WireServerMessage) -> Result<(), TransportError> {
            self.sent.push(msg);
            Ok(())
        }

        async fn receive(&mut self) -> Result<Option<ClientMessage>, TransportError> {
            Ok(None)
        }

        async fn receive_all(&mut self) -> Result<Vec<ClientMessage>, TransportError> {
            Ok(Vec::new())
        }

        async fn close(&mut self) -> Result<(), TransportError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct FailingOnceTransport {
        inner: CollectingTransport,
        failed: bool,
    }

    impl ServerTransport for FailingOnceTransport {
        async fn send(&mut self, msg: WireServerMessage) -> Result<(), TransportError> {
            if !self.failed {
                self.failed = true;
                return Err(TransportError::Other("synthetic send failure".into()));
            }
            self.inner.send(msg).await
        }

        async fn receive(&mut self) -> Result<Option<ClientMessage>, TransportError> {
            self.inner.receive().await
        }

        async fn receive_all(&mut self) -> Result<Vec<ClientMessage>, TransportError> {
            self.inner.receive_all().await
        }

        async fn close(&mut self) -> Result<(), TransportError> {
            self.inner.close().await
        }
    }

    struct FailingSequenceTransport {
        inner: CollectingTransport,
        fail_sequence: u32,
    }

    impl FailingSequenceTransport {
        fn new(fail_sequence: u32) -> Self {
            Self {
                inner: CollectingTransport::default(),
                fail_sequence,
            }
        }
    }

    impl ServerTransport for FailingSequenceTransport {
        async fn send(&mut self, msg: WireServerMessage) -> Result<(), TransportError> {
            if !matches!(msg.msg, lpc_wire::server::ServerMsgBody::ProjectRead { .. }) {
                return self.inner.send(msg).await;
            }
            if msg.seq == self.fail_sequence {
                return Err(TransportError::Other("synthetic sequence failure".into()));
            }
            self.inner.send(msg).await
        }

        async fn receive(&mut self) -> Result<Option<ClientMessage>, TransportError> {
            self.inner.receive().await
        }

        async fn receive_all(&mut self) -> Result<Vec<ClientMessage>, TransportError> {
            self.inner.receive_all().await
        }

        async fn close(&mut self) -> Result<(), TransportError> {
            self.inner.close().await
        }
    }

    /// Assert envelope id/seq and event count. Sequencing and finality now live
    /// on the envelope (`msg.seq`/`msg.fin`), not inside the body.
    fn assert_frame(message: &WireServerMessage, id: u64, sequence: u32, events: usize) {
        assert_eq!(message.id, id);
        assert_eq!(message.seq, sequence);
        let lpc_wire::server::ServerMsgBody::ProjectRead { events: sent } = &message.msg else {
            panic!("expected project-read events body");
        };
        assert_eq!(sent.len(), events);
    }

    /// Assert the envelope's finality flag on a sent frame.
    fn assert_fin(message: &WireServerMessage, fin: bool) {
        assert_eq!(
            message.fin, fin,
            "unexpected finality on frame {}",
            message.seq
        );
    }

    /// Measure with the same wire serializer the sink budgets against so the
    /// computed budgets in these tests match the sink's internal measurement.
    /// Non-final (`fin = false`) is the worst-case envelope the sink budgets
    /// against, so budget-derived test sizes must use it too.
    fn encoded_project_read_frame_len(
        id: u64,
        sequence: u32,
        events: &[ProjectReadEvent],
    ) -> usize {
        lpc_wire::ser_write_json_len(&WireServerMessage::stream_frame(
            id,
            sequence,
            false,
            lpc_wire::server::ServerMsgBody::ProjectRead {
                events: events.to_vec(),
            },
        ))
    }

    fn block_on<F: Future>(future: F) -> F::Output {
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        let mut future = Box::pin(future);
        loop {
            match Future::poll(Pin::as_mut(&mut future), &mut cx) {
                Poll::Ready(output) => return output,
                Poll::Pending => {}
            }
        }
    }

    fn noop_waker() -> Waker {
        unsafe fn clone(_: *const ()) -> RawWaker {
            RawWaker::new(core::ptr::null(), &VTABLE)
        }
        unsafe fn wake(_: *const ()) {}
        unsafe fn wake_by_ref(_: *const ()) {}
        unsafe fn drop(_: *const ()) {}
        static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake_by_ref, drop);

        unsafe { Waker::from_raw(RawWaker::new(core::ptr::null(), &VTABLE)) }
    }
}
