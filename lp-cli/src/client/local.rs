//! Async local transport for in-memory communication
//!
//! Provides tokio channel-based transport for communication between server and client
//! running in the same process. Uses unbounded channels for simplicity.

use crate::client::transport::ClientTransport;
use lp_model::{ClientMessage, ServerMessage, TransportError};
use lp_shared::transport::ServerTransport;
use tokio::sync::mpsc;

/// Async local client transport
///
/// Uses tokio channels to send client messages and receive server messages.
/// Provides async receive via `receive()`.
pub struct AsyncLocalClientTransport {
    /// Sender for client messages (client -> server)
    client_tx: Option<mpsc::UnboundedSender<ClientMessage>>,
    /// Receiver for server messages (server -> client)
    client_rx: mpsc::UnboundedReceiver<ServerMessage>,
    /// Whether the transport is closed
    closed: bool,
}

impl AsyncLocalClientTransport {
    /// Create a new async local client transport
    ///
    /// # Arguments
    ///
    /// * `client_tx` - Sender for client messages
    /// * `client_rx` - Receiver for server messages
    pub fn new(
        client_tx: mpsc::UnboundedSender<ClientMessage>,
        client_rx: mpsc::UnboundedReceiver<ServerMessage>,
    ) -> Self {
        Self {
            client_tx: Some(client_tx),
            client_rx,
            closed: false,
        }
    }
}

#[async_trait::async_trait]
impl ClientTransport for AsyncLocalClientTransport {
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

        self.client_rx
            .recv()
            .await
            .ok_or(TransportError::ConnectionLost)
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        if self.closed {
            return Ok(());
        }

        self.closed = true;
        // Drop the sender to signal closure to the other side
        self.client_tx = None;
        Ok(())
    }
}

/// Async local server transport
///
/// Uses tokio channels to send server messages and receive client messages.
/// Provides async receive via `receive()`.
pub struct AsyncLocalServerTransport {
    /// Sender for server messages (server -> client)
    server_tx: Option<mpsc::UnboundedSender<ServerMessage>>,
    /// Receiver for client messages (client -> server)
    server_rx: mpsc::UnboundedReceiver<ClientMessage>,
    /// Whether the transport is closed
    closed: bool,
}

impl AsyncLocalServerTransport {
    /// Create a new async local server transport
    ///
    /// # Arguments
    ///
    /// * `server_tx` - Sender for server messages
    /// * `server_rx` - Receiver for client messages
    pub fn new(
        server_tx: mpsc::UnboundedSender<ServerMessage>,
        server_rx: mpsc::UnboundedReceiver<ClientMessage>,
    ) -> Self {
        Self {
            server_tx: Some(server_tx),
            server_rx,
            closed: false,
        }
    }

    /// Receive a client message (async, blocking)
    ///
    /// Waits until a message is available or the connection is closed.
    #[allow(
        dead_code,
        reason = "Will be used in future async server implementations"
    )]
    pub async fn receive(&mut self) -> Result<Option<ClientMessage>, TransportError> {
        if self.closed {
            return Err(TransportError::ConnectionLost);
        }

        Ok(self.server_rx.recv().await)
    }

    /// Send a server message
    #[allow(
        dead_code,
        reason = "Will be used in future async server implementations"
    )]
    pub fn send(&mut self, msg: ServerMessage) -> Result<(), TransportError> {
        if self.closed {
            return Err(TransportError::ConnectionLost);
        }

        match &self.server_tx {
            Some(tx) => tx.send(msg).map_err(|_| TransportError::ConnectionLost),
            None => Err(TransportError::ConnectionLost),
        }
    }

    /// Close the transport
    #[allow(
        dead_code,
        reason = "Will be used in future async server implementations"
    )]
    pub fn close(&mut self) -> Result<(), TransportError> {
        if self.closed {
            return Ok(());
        }

        self.closed = true;
        // Drop the sender to signal closure to the other side
        self.server_tx = None;
        Ok(())
    }
}

impl ServerTransport for AsyncLocalServerTransport {
    fn send(&mut self, msg: ServerMessage) -> Result<(), TransportError> {
        if self.closed {
            return Err(TransportError::ConnectionLost);
        }

        match &self.server_tx {
            Some(tx) => tx.send(msg).map_err(|_| TransportError::ConnectionLost),
            None => Err(TransportError::ConnectionLost),
        }
    }

    fn receive(&mut self) -> Result<Option<ClientMessage>, TransportError> {
        if self.closed {
            return Err(TransportError::ConnectionLost);
        }

        // Use non-blocking try_recv() since ServerTransport::receive() is sync and non-blocking
        match self.server_rx.try_recv() {
            Ok(msg) => Ok(Some(msg)),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => Ok(None),
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                // Channel disconnected - mark as closed and return error
                self.closed = true;
                Err(TransportError::ConnectionLost)
            }
        }
    }

    fn close(&mut self) -> Result<(), TransportError> {
        if self.closed {
            return Ok(());
        }

        self.closed = true;
        // Drop the sender to signal closure to the other side
        self.server_tx = None;
        Ok(())
    }
}

/// Create a pair of connected local transports
///
/// Returns a client transport and server transport that are connected via
/// tokio channels. Messages sent via the client transport will be received
/// by the server transport, and vice versa.
pub fn create_local_transport_pair() -> (AsyncLocalClientTransport, AsyncLocalServerTransport) {
    let (client_tx, server_rx) = mpsc::unbounded_channel();
    let (server_tx, client_rx) = mpsc::unbounded_channel();

    (
        AsyncLocalClientTransport::new(client_tx, client_rx),
        AsyncLocalServerTransport::new(server_tx, server_rx),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_model::{ClientRequest, ServerMessage};

    #[tokio::test]
    async fn test_create_transport_pair() {
        let (_client_transport, _server_transport) = create_local_transport_pair();
        // Just verify they can be created
    }

    #[tokio::test]
    async fn test_client_send_receive() {
        let (mut client_transport, mut server_transport) = create_local_transport_pair();

        // Send message from client
        let client_msg = ClientMessage {
            id: 1,
            msg: ClientRequest::ListAvailableProjects,
        };
        client_transport.send(client_msg.clone()).await.unwrap();

        // Receive on server
        let received = server_transport.receive().await.unwrap();
        assert!(received.is_some());
        let received_msg = received.unwrap();
        assert_eq!(received_msg.id, 1);

        // Send response from server
        let server_msg = ServerMessage {
            id: 1,
            msg: lp_model::server::ServerMsgBody::ListAvailableProjects { projects: vec![] },
        };
        server_transport.send(server_msg).unwrap();

        // Receive on client
        let received = client_transport.receive().await.unwrap();
        assert_eq!(received.id, 1);
    }
}
