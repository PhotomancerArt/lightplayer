//! Async serial client transport
//!
//! Generic async serial transport that uses channels for communication.
//! Works with both emulator and hardware serial (future) via factory functions.

use crate::transport::ClientTransport;
use lpc_wire::WireServerMessage;
use lpc_wire::{TransportError, messages::ClientMessage};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};

/// Async serial client transport
///
/// Generic transport that uses channels for communication with a serial backend
/// running on a separate thread. The backend can be emulator or hardware serial.
///
/// This transport is generic and doesn't know about the implementation details -
/// only the factory functions know about emulator vs hardware.
pub struct AsyncSerialClientTransport {
    /// Sender for client messages (client -> backend thread)
    client_tx: Option<mpsc::UnboundedSender<ClientMessage>>,
    /// Receiver for what the backend read (backend thread -> client)
    server_rx: mpsc::UnboundedReceiver<SerialInbound>,
    /// The backend's link session number, bumped by the backend on every
    /// link reset (see [`SerialInbound::LinkReset`]). Always 0 for a backend
    /// with no link (the fw-emu syscall pipe).
    link_generation: Arc<AtomicU32>,
    /// The link session the last request was sent in, until a reset has
    /// failed it.
    last_send_generation: Option<u32>,
    /// Shutdown signal sender (client -> backend thread)
    shutdown_tx: Option<oneshot::Sender<()>>,
    /// Handle to the backend thread
    thread_handle: Option<JoinHandle<()>>,
    /// Whether the transport is closed
    closed: bool,
    /// What the backend thread is talking to (a port name, `emulator`, …).
    /// Names the resource in the close-timeout error, which is otherwise a
    /// mystery "resource busy" one layer up.
    backend_label: String,
}

/// How long [`ClientTransport::close`] waits for the backend thread to exit
/// and release whatever it owns.
///
/// Generous against the backend's own bounds, not a guess: the hardware
/// loop's slowest pass is one 100 ms read timeout plus a bounded write, and
/// it re-checks the shutdown signal between writes. Anything past this is a
/// backend wedged inside a blocking call, which is a bug in that backend —
/// see `docs/defects/2026-09-08-serial-close-leaks-the-port-on-a-wedged-device.md`.
pub(crate) const CLOSE_JOIN_BUDGET: Duration = Duration::from_secs(2);

/// What a backend thread hands the transport.
#[derive(Debug)]
pub(crate) enum SerialInbound {
    /// One message from the board.
    Message(WireServerMessage),
    /// The link reset (now in session `generation`) with requests in flight:
    /// they are lost. Fails a `receive` whose request went out before the
    /// reset, and is skipped otherwise (D9, [`crate::link_reset`]).
    LinkReset { generation: u32, detail: String },
    /// One request could not be sent at all (too big for the link): fails
    /// the `receive` waiting for its answer.
    SendFailed(String),
}

impl AsyncSerialClientTransport {
    /// Create a new async serial client transport
    ///
    /// This is an internal constructor used by factory functions.
    /// Use `create_emulator_serial_transport_pair()` or similar factory functions instead.
    ///
    /// # Arguments
    ///
    /// * `client_tx` - Sender for client messages
    /// * `server_rx` - Receiver for what the backend read
    /// * `link_generation` - The backend's link session number
    /// * `shutdown_tx` - Shutdown signal sender
    /// * `thread_handle` - Handle to the backend thread
    /// * `backend_label` - What the thread talks to, for close diagnostics
    #[cfg(any(feature = "serial", test))]
    pub(crate) fn new(
        client_tx: mpsc::UnboundedSender<ClientMessage>,
        server_rx: mpsc::UnboundedReceiver<SerialInbound>,
        link_generation: Arc<AtomicU32>,
        shutdown_tx: oneshot::Sender<()>,
        thread_handle: JoinHandle<()>,
        backend_label: impl Into<String>,
    ) -> Self {
        Self {
            client_tx: Some(client_tx),
            server_rx,
            link_generation,
            last_send_generation: None,
            shutdown_tx: Some(shutdown_tx),
            thread_handle: Some(thread_handle),
            closed: false,
            backend_label: backend_label.into(),
        }
    }
}

#[async_trait::async_trait]
impl ClientTransport for AsyncSerialClientTransport {
    async fn send(&mut self, msg: ClientMessage) -> Result<(), TransportError> {
        if self.closed {
            return Err(TransportError::ConnectionLost);
        }

        self.last_send_generation = Some(self.link_generation.load(Ordering::SeqCst));
        match &self.client_tx {
            Some(tx) => tx.send(msg).map_err(|_| TransportError::ConnectionLost),
            None => Err(TransportError::ConnectionLost),
        }
    }

    async fn receive(&mut self) -> Result<WireServerMessage, TransportError> {
        if self.closed {
            return Err(TransportError::ConnectionLost);
        }

        loop {
            match self.server_rx.recv().await {
                None => return Err(TransportError::ConnectionLost),
                Some(SerialInbound::Message(message)) => return Ok(message),
                Some(SerialInbound::SendFailed(detail)) => {
                    return Err(TransportError::Other(detail));
                }
                // Only a request sent before the reset was lost with it; a
                // reset nobody is waiting on is skipped, so it cannot fail a
                // request sent after it.
                Some(SerialInbound::LinkReset { generation, detail }) => {
                    if self
                        .last_send_generation
                        .is_some_and(|sent| sent < generation)
                    {
                        self.last_send_generation = None;
                        return Err(crate::link_reset::link_reset_error(&detail));
                    }
                }
            }
        }
    }

    /// Shut the backend thread down and WAIT for it to exit.
    ///
    /// The wait is the contract, not politeness: the backend thread is the
    /// sole owner of the byte stream, so returning `Ok` here is what
    /// promises that the OS resource behind it (a serial port) is free for
    /// the next holder — a management operation reopens the same port
    /// immediately after this returns. An `Err` therefore means the resource
    /// is still held, and callers must surface it rather than treat close as
    /// best-effort.
    async fn close(&mut self) -> Result<(), TransportError> {
        if self.closed {
            return Ok(());
        }

        self.closed = true;

        // Send shutdown signal
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }

        // Drop client_tx to signal closure to backend thread
        self.client_tx = None;

        // Wait for thread to finish (with timeout)
        if let Some(handle) = self.thread_handle.take() {
            let start = Instant::now();
            loop {
                if handle.is_finished() {
                    handle.join().map_err(|_| {
                        TransportError::Other("Backend thread panicked".to_string())
                    })?;
                    break;
                }
                if start.elapsed() > CLOSE_JOIN_BUDGET {
                    let message = format!(
                        "serial backend thread for {} did not stop within {:.1}s; \
                         it still holds the connection",
                        self.backend_label,
                        CLOSE_JOIN_BUDGET.as_secs_f64()
                    );
                    log::error!("{message}");
                    return Err(TransportError::Other(message));
                }
                // Runtime-neutral wait: `close` must work without a tokio
                // reactor (DeviceSession drives it from single-actor edges).
                // Blocking briefly is fine — this waits for an OS thread to
                // exit, bounded by the budget above.
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        Ok(())
    }
}

impl Drop for AsyncSerialClientTransport {
    fn drop(&mut self) {
        // If not already closed, try to close (best-effort)
        if !self.closed {
            // Mark as closed
            self.closed = true;

            // Send shutdown signal
            if let Some(shutdown_tx) = self.shutdown_tx.take() {
                let _ = shutdown_tx.send(());
            }

            // Drop client_tx
            self.client_tx = None;

            // Try to join thread (with short timeout to avoid hanging in Drop)
            if let Some(handle) = self.thread_handle.take() {
                let start = Instant::now();
                loop {
                    if handle.is_finished() {
                        let _ = handle.join();
                        break;
                    }
                    if start.elapsed() > Duration::from_millis(100) {
                        // Timeout - don't wait forever in Drop. The thread
                        // still owns the byte stream, so whatever it holds
                        // stays held; say so, because the symptom one layer
                        // up is an unexplained "resource busy".
                        log::warn!(
                            "serial backend thread for {} still running after drop; \
                             its connection is still held",
                            self.backend_label
                        );
                        break;
                    }
                    std::thread::yield_now();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_transport_creation() {
        // Create dummy channels and thread handle
        let (client_tx, _client_rx) = mpsc::unbounded_channel::<ClientMessage>();
        let (_server_tx, server_rx) = mpsc::unbounded_channel::<SerialInbound>();
        let (shutdown_tx, _shutdown_rx) = oneshot::channel();

        // Create a dummy thread that just exits immediately
        let thread_handle = std::thread::spawn(|| {});

        let mut transport = AsyncSerialClientTransport::new(
            client_tx,
            server_rx,
            Arc::new(AtomicU32::new(0)),
            shutdown_tx,
            thread_handle,
            "test",
        );

        // Verify we can call close
        transport.close().await.unwrap();
    }

    /// A reset fails the request that went out before it, and only that one:
    /// a reset nobody was waiting on does not fail the next request.
    #[tokio::test]
    async fn a_link_reset_fails_only_a_request_sent_before_it() {
        let (client_tx, _client_rx) = mpsc::unbounded_channel::<ClientMessage>();
        let (server_tx, server_rx) = mpsc::unbounded_channel::<SerialInbound>();
        let (shutdown_tx, _shutdown_rx) = oneshot::channel();
        let generation = Arc::new(AtomicU32::new(0));
        let mut transport = AsyncSerialClientTransport::new(
            client_tx,
            server_rx,
            Arc::clone(&generation),
            shutdown_tx,
            std::thread::spawn(|| {}),
            "test",
        );
        let hello = |id| ClientMessage {
            id,
            msg: lpc_wire::ClientRequest::Hello,
        };

        transport.send(hello(1)).await.unwrap();
        generation.store(1, Ordering::SeqCst);
        server_tx
            .send(SerialInbound::LinkReset {
                generation: 1,
                detail: "board restarted".to_string(),
            })
            .unwrap();
        let error = transport.receive().await.unwrap_err();
        assert!(crate::is_link_reset(&error), "{error}");

        // The next request goes out in session 1; a late notice of the
        // reset into session 1 is not its failure.
        transport.send(hello(2)).await.unwrap();
        server_tx
            .send(SerialInbound::LinkReset {
                generation: 1,
                detail: "board restarted".to_string(),
            })
            .unwrap();
        server_tx
            .send(SerialInbound::Message(WireServerMessage::new(
                2,
                lpc_wire::ServerMsgBody::UnloadProject,
            )))
            .unwrap();
        assert_eq!(transport.receive().await.unwrap().id, 2);
        transport.close().await.unwrap();
    }
}
