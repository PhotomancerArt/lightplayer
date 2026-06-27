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
    /// Send a server message (consumes the message)
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

        let frame = ProjectReadFrame::new(self.sequence, core::mem::take(&mut self.pending_events));
        self.sequence = self.sequence.saturating_add(1);
        self.transport
            .send(WireServerMessage {
                id: self.id,
                msg: lpc_wire::server::ServerMsgBody::ProjectReadFrame { frame },
            })
            .await
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
