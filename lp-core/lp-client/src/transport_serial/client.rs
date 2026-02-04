//! Async serial client transport
//!
//! Generic async serial transport that uses channels for communication.
//! Works with both emulator and hardware serial (future) via factory functions.

use crate::transport::ClientTransport;
use lp_model::{ClientMessage, ServerMessage, TransportError};
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
    /// Receiver for server messages (backend thread -> client)
    server_rx: mpsc::UnboundedReceiver<ServerMessage>,
    /// Shutdown signal sender (client -> backend thread)
    shutdown_tx: Option<oneshot::Sender<()>>,
    /// Handle to the backend thread
    thread_handle: Option<JoinHandle<()>>,
    /// Whether the transport is closed
    closed: bool,
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
    /// * `server_rx` - Receiver for server messages
    /// * `shutdown_tx` - Shutdown signal sender
    /// * `thread_handle` - Handle to the backend thread
    pub(crate) fn new(
        client_tx: mpsc::UnboundedSender<ClientMessage>,
        server_rx: mpsc::UnboundedReceiver<ServerMessage>,
        shutdown_tx: oneshot::Sender<()>,
        thread_handle: JoinHandle<()>,
    ) -> Self {
        Self {
            client_tx: Some(client_tx),
            server_rx,
            shutdown_tx: Some(shutdown_tx),
            thread_handle: Some(thread_handle),
            closed: false,
        }
    }
}

#[async_trait::async_trait]
impl ClientTransport for AsyncSerialClientTransport {
    async fn send(&mut self, msg: ClientMessage) -> Result<(), TransportError> {
        if self.closed {
            return Err(TransportError::ConnectionLost);
        }

        match &self.client_tx {
            Some(tx) => tx.send(msg).map_err(|_| TransportError::ConnectionLost),
            None => Err(TransportError::ConnectionLost),
        }
    }

    async fn receive(&mut self) -> Result<ServerMessage, TransportError> {
        if self.closed {
            return Err(TransportError::ConnectionLost);
        }

        self.server_rx
            .recv()
            .await
            .ok_or(TransportError::ConnectionLost)
    }

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
                if start.elapsed() > Duration::from_secs(1) {
                    return Err(TransportError::Other(
                        "Backend thread did not stop within timeout".to_string(),
                    ));
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
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
                        // Timeout - don't wait forever in Drop
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
        let (_server_tx, server_rx) = mpsc::unbounded_channel::<ServerMessage>();
        let (shutdown_tx, _shutdown_rx) = oneshot::channel();

        // Create a dummy thread that just exits immediately
        let thread_handle = std::thread::spawn(|| {});

        let mut transport =
            AsyncSerialClientTransport::new(client_tx, server_rx, shutdown_tx, thread_handle);

        // Verify we can call close
        transport.close().await.unwrap();
    }
}
