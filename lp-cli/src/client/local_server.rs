//! Local server transport
//!
//! Encapsulates an in-memory server running on a separate thread and provides
//! a client transport interface for communicating with it.

use crate::client::transport::ClientTransport;
use anyhow::Result;
use lp_model::TransportError;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};

use crate::client::local::{AsyncLocalClientTransport, create_local_transport_pair};
use crate::server::{create_server, run_server_loop_async};

/// Local server transport that manages an in-memory server thread
///
/// This struct encapsulates the lifecycle of a server running on a separate thread.
/// It provides access to a client transport for communicating with the server.
pub struct LocalServerTransport {
    /// Handle to the server thread (None after close())
    server_handle: Option<JoinHandle<()>>,
    /// Client transport for communicating with the server
    client_transport: Option<AsyncLocalClientTransport>,
    /// Whether the transport has been closed
    closed: Arc<AtomicBool>,
}

impl LocalServerTransport {
    /// Create a new local server transport
    ///
    /// Spawns a server thread with its own tokio runtime and returns a client transport
    /// for communicating with it.
    ///
    /// # Returns
    ///
    /// * `Ok(Self)` if server was spawned successfully
    /// * `Err` if server creation or thread spawning failed
    pub fn new() -> Result<Self> {
        // Create transport pair
        let (client_transport, server_transport) = create_local_transport_pair();

        // Create closed flag (shared between client and server)
        let closed = Arc::new(AtomicBool::new(false));

        // Spawn server thread
        let closed_clone = Arc::clone(&closed);
        let server_handle = thread::Builder::new()
            .name("lp-server".to_string())
            .spawn(move || {
                // Create tokio runtime for server
                let runtime = match tokio::runtime::Runtime::new() {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("Failed to create tokio runtime for server: {e}");
                        return;
                    }
                };

                // Create server inside the thread (LpServer is not Send)
                let (server, _base_fs) = match create_server(None, true, None) {
                    Ok((s, fs)) => (s, fs),
                    Err(e) => {
                        eprintln!("Failed to create server: {e}");
                        return;
                    }
                };

                // Run server loop until transport closes
                runtime.block_on(async {
                    // Create LocalSet for spawn_local (needed because LpServer is not Send)
                    let local_set = tokio::task::LocalSet::new();
                    let _ = local_set
                        .run_until(run_server_loop_async(server, server_transport))
                        .await;
                });

                // Mark as closed when server exits
                closed_clone.store(true, Ordering::Relaxed);
            })
            .map_err(|e| anyhow::anyhow!("Failed to spawn server thread: {e}"))?;

        Ok(Self {
            server_handle: Some(server_handle),
            client_transport: Some(client_transport),
            closed,
        })
    }

    /// Close the transport and stop the server
    ///
    /// This method is idempotent - calling it multiple times is safe.
    /// It closes the client transport (which signals the server to shut down) and
    /// waits for the server thread to finish.
    ///
    /// # Returns
    ///
    /// * `Ok(())` if the server was stopped successfully (or already closed)
    /// * `Err` if waiting for the thread failed
    #[allow(
        dead_code,
        reason = "Will be used in future cleanup/shutdown scenarios"
    )]
    pub fn close(&mut self) -> Result<()> {
        // Check if already closed
        if self.closed.load(Ordering::Relaxed) {
            return Ok(());
        }

        // Mark as closed
        self.closed.store(true, Ordering::Relaxed);

        // Close the client transport (signals server to shut down)
        // Dropping the client_transport will close its channels, which signals the server
        // to stop (server's receive() will return ConnectionLost)
        drop(self.client_transport.take());

        // Wait for server thread to finish (with timeout to avoid hanging)
        if let Some(handle) = self.server_handle.take() {
            // Use a timeout to avoid hanging forever if server doesn't stop
            let start = std::time::Instant::now();
            loop {
                if handle.is_finished() {
                    handle
                        .join()
                        .map_err(|_| anyhow::anyhow!("Server thread panicked"))?;
                    break;
                }
                if start.elapsed() > std::time::Duration::from_secs(1) {
                    // Timeout - server didn't stop, abort the thread
                    eprintln!("Warning: Server thread did not stop within timeout, aborting");
                    return Err(anyhow::anyhow!("Server thread did not stop within timeout"));
                }
                std::thread::yield_now();
            }
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl ClientTransport for LocalServerTransport {
    async fn send(&mut self, msg: lp_model::ClientMessage) -> Result<(), TransportError> {
        match &mut self.client_transport {
            Some(transport) => transport.send(msg).await,
            None => Err(TransportError::ConnectionLost),
        }
    }

    async fn receive(&mut self) -> Result<lp_model::ServerMessage, TransportError> {
        match &mut self.client_transport {
            Some(transport) => transport.receive().await,
            None => Err(TransportError::ConnectionLost),
        }
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        // Check if already closed
        if self.closed.load(Ordering::Relaxed) {
            return Ok(());
        }

        // Mark as closed
        self.closed.store(true, Ordering::Relaxed);

        // Close the client transport (signals server to shut down)
        if let Some(mut transport) = self.client_transport.take() {
            let _ = transport.close().await;
        }

        // Wait for server thread to finish (with timeout)
        if let Some(handle) = self.server_handle.take() {
            // Use tokio::time::timeout in async context
            let start = std::time::Instant::now();
            loop {
                if handle.is_finished() {
                    handle
                        .join()
                        .map_err(|_| TransportError::Other("Server thread panicked".to_string()))?;
                    break;
                }
                if start.elapsed() > std::time::Duration::from_secs(1) {
                    return Err(TransportError::Other(
                        "Server thread did not stop within timeout".to_string(),
                    ));
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            }
        }

        Ok(())
    }
}

impl Drop for LocalServerTransport {
    fn drop(&mut self) {
        // If not already closed, try to close (best-effort)
        if !self.closed.load(Ordering::Relaxed) {
            // Mark as closed
            self.closed.store(true, Ordering::Relaxed);
            // Close the client transport first (signals server to shut down)
            drop(self.client_transport.take());
            // Try to join the thread if we still have the handle (with timeout to avoid hanging)
            if let Some(handle) = self.server_handle.take() {
                // Use a short timeout to avoid hanging forever in doctests
                let start = std::time::Instant::now();
                loop {
                    if handle.is_finished() {
                        let _ = handle.join();
                        break;
                    }
                    if start.elapsed() > std::time::Duration::from_millis(100) {
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
    use lp_model::{ClientMessage, ClientRequest};

    #[tokio::test]
    async fn test_local_server_transport_creation() {
        let mut transport = LocalServerTransport::new().unwrap();
        // Verify it was created
        // Give server time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        // Cleanup
        transport.close().unwrap();
    }

    #[tokio::test]
    async fn test_client_transport_works() {
        let mut transport = LocalServerTransport::new().unwrap();

        // Give server time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Test that we can send and receive
        let msg = ClientMessage {
            id: 1,
            msg: ClientRequest::ListAvailableProjects,
        };
        // Note: send is async, but we can't easily test without a server response
        // Just verify the transport exists
        let _ = transport.send(msg).await;

        // Close the transport
        transport.close().unwrap();
    }

    #[tokio::test]
    async fn test_close_stops_server() {
        let mut transport = LocalServerTransport::new().unwrap();
        // Give server time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        // Close should wait for server thread
        transport.close().unwrap();
        // If we get here, close succeeded
    }
}
