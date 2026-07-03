//! Server-side transport trait
//!
//! Defines the interface for server-side transport implementations.
//! Messages are consumed (moved) on send, and receive is non-blocking.
//!
//! The transport handles serialization/deserialization internally.
//! Project reads use [`ProjectReadFrameSink`] so every transport shares the
//! same bounded `ProjectReadFrame` batching policy.

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use lpc_wire::{
    PROJECT_READ_FRAME_MAX_BYTES, ProjectReadEvent, ProjectReadFrame, TransportError,
    WireServerMessage, messages::ClientMessage,
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

/// Shared project-read event batcher used by all server transports.
pub struct ProjectReadFrameSink<'a, T> {
    transport: &'a mut T,
    id: u64,
    sequence: u32,
    pending_events: Vec<ProjectReadEvent>,
    max_bytes: usize,
}

impl<'a, T> ProjectReadFrameSink<'a, T>
where
    T: ServerTransport,
{
    pub fn new(transport: &'a mut T, id: u64) -> Self {
        Self::with_max_bytes(transport, id, PROJECT_READ_FRAME_MAX_BYTES)
    }

    pub fn with_max_bytes(transport: &'a mut T, id: u64, max_bytes: usize) -> Self {
        Self {
            transport,
            id,
            sequence: 0,
            pending_events: Vec::new(),
            max_bytes,
        }
    }

    pub async fn finish(&mut self) -> Result<(), TransportError> {
        self.flush().await
    }

    /// Best-effort emit a terminal [`ProjectReadEvent::Error`] frame.
    ///
    /// Called when the event stream failed with a *signalable* error (an event
    /// too large for an empty frame, or another serialization/budget failure).
    /// Any partially batched events are discarded — the stream is already
    /// broken — and a standalone `Error` frame is sent at the current sequence
    /// so the client sees a terminal failure for this request id instead of a
    /// silent stall. A transport-write failure while emitting this frame is
    /// returned to the caller but is expected to be logged and ignored.
    pub async fn send_terminal_error(&mut self, message: String) -> Result<(), TransportError> {
        // Drop any pending batch; those events could not be delivered anyway.
        self.pending_events.clear();
        let sequence = self.sequence;
        let frame = ProjectReadFrame::new(sequence, vec![ProjectReadEvent::Error { message }]);
        self.transport
            .send(WireServerMessage {
                id: self.id,
                msg: lpc_wire::server::ServerMsgBody::ProjectReadFrame { frame },
            })
            .await?;
        self.sequence = self.sequence.saturating_add(1);
        Ok(())
    }

    async fn push_event(&mut self, event: ProjectReadEvent) -> Result<(), TransportError> {
        let mut candidate = self.pending_events.clone();
        candidate.push(event.clone());
        if self.encoded_frame_len(self.sequence, candidate.as_slice())? <= self.max_bytes {
            self.pending_events.push(event);
            return Ok(());
        }

        if !self.pending_events.is_empty() {
            self.flush().await?;
        }

        let single = [event.clone()];
        if self.encoded_frame_len(self.sequence, &single)? > self.max_bytes {
            return Err(TransportError::Serialization(format!(
                "project-read event exceeded frame budget of {} bytes",
                self.max_bytes
            )));
        }

        self.pending_events.push(event);
        Ok(())
    }

    async fn flush(&mut self) -> Result<(), TransportError> {
        if self.pending_events.is_empty() {
            return Ok(());
        }

        let sequence = self.sequence;
        let events = core::mem::take(&mut self.pending_events);
        let frame = ProjectReadFrame::new(sequence, events.clone());
        match self
            .transport
            .send(WireServerMessage {
                id: self.id,
                msg: lpc_wire::server::ServerMsgBody::ProjectReadFrame { frame },
            })
            .await
        {
            Ok(()) => {
                self.sequence = self.sequence.saturating_add(1);
                Ok(())
            }
            Err(error) => {
                self.pending_events = events;
                Err(error)
            }
        }
    }

    fn encoded_frame_len(
        &self,
        sequence: u32,
        events: &[ProjectReadEvent],
    ) -> Result<usize, TransportError> {
        let message = WireServerMessage {
            id: self.id,
            msg: lpc_wire::server::ServerMsgBody::ProjectReadFrame {
                frame: ProjectReadFrame::new(sequence, events.to_vec()),
            },
        };
        lpc_wire::json::to_string(&message)
            .map(|json| json.len())
            .map_err(|error| TransportError::Serialization(format!("{error}")))
    }
}

impl<T> ProjectReadEventSink for ProjectReadFrameSink<'_, T>
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
            let mut sink = ProjectReadFrameSink::with_max_bytes(&mut transport, 9, max_bytes);
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
        let max_bytes = encoded_project_read_frame_len(9, 0, &[begin.clone()]);
        let mut transport = CollectingTransport::default();

        block_on(async {
            let mut sink = ProjectReadFrameSink::with_max_bytes(&mut transport, 9, max_bytes);
            sink.send_project_read_event(begin).await.unwrap();
            sink.send_project_read_event(end).await.unwrap();
            sink.finish().await.unwrap();
        });

        assert_eq!(transport.sent.len(), 2);
        assert_frame(&transport.sent[0], 9, 0, 1);
        assert_frame(&transport.sent[1], 9, 1, 1);
    }

    #[test]
    fn frame_sink_finish_flushes_partial_final_frame() {
        let event = ProjectReadEvent::Begin {
            revision: Revision::new(7),
        };
        let max_bytes = encoded_project_read_frame_len(9, 0, &[event.clone()]) * 2;
        let mut transport = CollectingTransport::default();

        block_on(async {
            let mut sink = ProjectReadFrameSink::with_max_bytes(&mut transport, 9, max_bytes);
            sink.send_project_read_event(event).await.unwrap();
            assert!(sink.transport.sent.is_empty());
            sink.finish().await.unwrap();
        });

        assert_eq!(transport.sent.len(), 1);
        assert_frame(&transport.sent[0], 9, 0, 1);
    }

    #[test]
    fn frame_sink_finish_does_not_emit_empty_frame() {
        let mut transport = CollectingTransport::default();

        block_on(async {
            let mut sink = ProjectReadFrameSink::with_max_bytes(&mut transport, 9, usize::MAX);
            sink.finish().await.unwrap();
        });

        assert!(transport.sent.is_empty());
    }

    #[test]
    fn frame_sink_rejects_oversized_single_event() {
        let event = ProjectReadEvent::Begin {
            revision: Revision::new(7),
        };
        let max_bytes = encoded_project_read_frame_len(9, 0, &[event.clone()]).saturating_sub(1);
        let mut transport = CollectingTransport::default();

        let error = block_on(async {
            let mut sink = ProjectReadFrameSink::with_max_bytes(&mut transport, 9, max_bytes);
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
            let mut sink = ProjectReadFrameSink::with_max_bytes(&mut transport, 9, max_bytes);
            // The oversized event cannot fit an empty frame: signalable failure.
            let error = sink.send_project_read_event(event).await.unwrap_err();
            assert!(transport_error_is_signalable(&error));
            // Best-effort terminal error frame for this request id.
            sink.send_terminal_error(alloc::format!("{error}"))
                .await
                .unwrap();
        });

        // The only thing the client receives is the terminal error frame.
        assert_eq!(transport.sent.len(), 1);
        assert_eq!(transport.sent[0].id, 9);
        let lpc_wire::server::ServerMsgBody::ProjectReadFrame { frame } = &transport.sent[0].msg
        else {
            panic!("expected project-read frame");
        };
        assert_eq!(frame.sequence, 0);
        assert_eq!(frame.events.len(), 1);
        assert!(matches!(frame.events[0], ProjectReadEvent::Error { .. }));
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
        let max_bytes = encoded_project_read_frame_len(9, 0, &[begin.clone()]);
        let mut transport = FailingOnceTransport::default();

        block_on(async {
            let mut sink = ProjectReadFrameSink::with_max_bytes(&mut transport, 9, max_bytes);
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
        let max_bytes = encoded_project_read_frame_len(9, 0, &[event.clone()]);
        let mut transport = FailingSequenceTransport::new(3);

        block_on(async {
            let mut sink = ProjectReadFrameSink::with_max_bytes(&mut transport, 9, max_bytes);
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
            let lpc_wire::server::ServerMsgBody::ProjectReadFrame { frame } = &msg.msg else {
                return self.inner.send(msg).await;
            };
            if frame.sequence == self.fail_sequence {
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

    fn assert_frame(message: &WireServerMessage, id: u64, sequence: u32, events: usize) {
        assert_eq!(message.id, id);
        let lpc_wire::server::ServerMsgBody::ProjectReadFrame { frame } = &message.msg else {
            panic!("expected project-read frame");
        };
        assert_eq!(frame.sequence, sequence);
        assert_eq!(frame.events.len(), events);
    }

    fn encoded_project_read_frame_len(
        id: u64,
        sequence: u32,
        events: &[ProjectReadEvent],
    ) -> usize {
        lpc_wire::json::to_string(&WireServerMessage {
            id,
            msg: lpc_wire::server::ServerMsgBody::ProjectReadFrame {
                frame: ProjectReadFrame::new(sequence, events.to_vec()),
            },
        })
        .unwrap()
        .len()
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
