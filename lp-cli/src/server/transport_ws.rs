//! WebSocket server transport
//!
//! Implements `ServerTransport` using async `tokio-tungstenite` for handling multiple
//! connections, wrapped in a polling interface using channels.

use std::collections::{HashMap, VecDeque};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use futures_util::SinkExt;
use futures_util::stream::StreamExt;
use lp_model::{ClientMessage, ServerMessage, TransportError};
use lp_shared::transport::ServerTransport;
use tokio::runtime::Runtime;
use tokio::sync::mpsc;

/// Connection ID for tracking multiple websocket connections
type ConnectionId = u64;

/// Connection state for a single websocket client
struct Connection {
    /// Channel sender for sending messages to this connection
    sender: mpsc::UnboundedSender<ServerMessage>,
    /// Channel receiver for receiving messages from this connection
    /// Note: Currently unused as messages are received via the async task,
    /// but kept for potential future use or debugging
    #[allow(dead_code, reason = "Reserved for future use or debugging")]
    receiver: mpsc::UnboundedReceiver<ClientMessage>,
}

/// Shared state for connection management (accessed from async tasks)
struct SharedState {
    /// Map of connection ID to connection state
    connections: HashMap<ConnectionId, Connection>,
    /// Queue of incoming messages from clients (connection_id, message)
    pending_messages: VecDeque<(ConnectionId, ClientMessage)>,
    /// Next connection ID to assign
    next_connection_id: ConnectionId,
}

/// WebSocket server transport
///
/// Uses async `tokio-tungstenite` internally but provides a sync polling interface.
/// Handles multiple simultaneous connections and routes messages appropriately.
pub struct WebSocketServerTransport {
    /// TCP listener for accepting new connections
    /// Note: Kept to maintain ownership of the listener, even though not directly accessed
    #[allow(dead_code, reason = "Maintains ownership of listener")]
    listener: TcpListener,
    /// Shared state for connection management
    shared_state: Arc<Mutex<SharedState>>,
    /// Tokio runtime for async operations
    /// Note: Kept to ensure runtime stays alive for async tasks
    #[allow(dead_code, reason = "Ensures runtime stays alive for async tasks")]
    runtime: Arc<Runtime>,
}

impl WebSocketServerTransport {
    /// Create a new WebSocket server transport and bind to the specified port
    ///
    /// # Arguments
    ///
    /// * `port` - Port to bind to (e.g., 2812)
    ///
    /// # Returns
    ///
    /// * `Ok(Self)` if binding succeeded
    /// * `Err(TransportError)` if binding failed
    pub fn new(port: u16) -> Result<Self, TransportError> {
        // Create tokio runtime
        let runtime = Runtime::new()
            .map_err(|e| TransportError::Other(format!("Failed to create tokio runtime: {e}")))?;

        // Bind TCP listener
        let addr = format!("0.0.0.0:{port}");
        let listener = TcpListener::bind(&addr)
            .map_err(|e| TransportError::Other(format!("Failed to bind to {addr}: {e}")))?;

        // Set non-blocking mode for the listener
        listener
            .set_nonblocking(true)
            .map_err(|e| TransportError::Other(format!("Failed to set non-blocking: {e}")))?;

        // Create shared state
        let shared_state = Arc::new(Mutex::new(SharedState {
            connections: HashMap::new(),
            pending_messages: VecDeque::new(),
            next_connection_id: 0,
        }));

        let runtime_arc = Arc::new(runtime);
        let runtime_clone = Arc::clone(&runtime_arc);
        let shared_state_clone = Arc::clone(&shared_state);

        // Start async task to accept connections
        runtime_clone.spawn(Self::accept_connections_task(
            listener
                .try_clone()
                .map_err(|e| TransportError::Other(format!("Failed to clone listener: {e}")))?,
            shared_state_clone,
        ));

        Ok(Self {
            listener,
            shared_state,
            runtime: runtime_arc,
        })
    }

    /// Async task to accept new websocket connections
    ///
    /// This runs in the tokio runtime and handles accepting new connections,
    /// upgrading them to websockets, and spawning tasks for each connection.
    async fn accept_connections_task(
        listener: std::net::TcpListener,
        shared_state: Arc<Mutex<SharedState>>,
    ) {
        use tokio::net::TcpListener as TokioTcpListener;
        use tokio_tungstenite::accept_async;

        // Convert std::net::TcpListener to tokio::net::TcpListener
        let listener = match TokioTcpListener::from_std(listener) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("Failed to convert listener to tokio: {e}");
                return;
            }
        };

        while let Ok((stream, _)) = listener.accept().await {
            // Upgrade to websocket
            match accept_async(stream).await {
                Ok(ws_stream) => {
                    // Get connection ID
                    let connection_id = {
                        let mut state = shared_state.lock().unwrap();
                        let id = state.next_connection_id;
                        state.next_connection_id += 1;
                        id
                    };

                    // Spawn task to handle this connection
                    tokio::spawn(Self::handle_connection(
                        ws_stream,
                        connection_id,
                        Arc::clone(&shared_state),
                    ));
                }
                Err(e) => {
                    eprintln!("Failed to accept websocket connection: {e}");
                }
            }
        }
    }

    /// Handle a single websocket connection
    ///
    /// This runs for each connected client and handles bidirectional communication.
    async fn handle_connection<S>(
        ws_stream: tokio_tungstenite::WebSocketStream<S>,
        connection_id: ConnectionId,
        shared_state: Arc<Mutex<SharedState>>,
    ) where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    {
        use tokio_tungstenite::tungstenite::Message;

        // Split into sender and receiver
        let (mut ws_sender, mut ws_receiver) = ws_stream.split();

        // Create channels for communication with sync code
        let (client_tx, mut client_rx) = mpsc::unbounded_channel::<ServerMessage>();
        let (_server_tx, server_rx) = mpsc::unbounded_channel::<ClientMessage>();

        // Register connection
        {
            let mut state = shared_state.lock().unwrap();
            state.connections.insert(
                connection_id,
                Connection {
                    sender: client_tx,
                    receiver: server_rx,
                },
            );
        }

        // Task to send messages from channel to websocket
        tokio::spawn(async move {
            while let Some(msg) = client_rx.recv().await {
                // Serialize ServerMessage to JSON
                let json = match serde_json::to_string(&msg) {
                    Ok(j) => j,
                    Err(e) => {
                        eprintln!("Failed to serialize ServerMessage: {e}");
                        continue;
                    }
                };

                // Send via websocket
                if let Err(e) = ws_sender.send(Message::Text(json)).await {
                    eprintln!("Failed to send message: {e}");
                    break;
                }
            }
        });

        // Task to receive messages from websocket and add to shared state
        tokio::spawn(async move {
            while let Some(msg_result) = ws_receiver.next().await {
                match msg_result {
                    Ok(Message::Text(text)) => {
                        // Deserialize ClientMessage from JSON
                        match serde_json::from_str::<ClientMessage>(&text) {
                            Ok(client_msg) => {
                                let mut state = shared_state.lock().unwrap();
                                state
                                    .pending_messages
                                    .push_back((connection_id, client_msg));
                            }
                            Err(e) => {
                                eprintln!("Failed to deserialize ClientMessage: {e}");
                            }
                        }
                    }
                    Ok(Message::Binary(data)) => {
                        // Deserialize from binary JSON
                        match serde_json::from_slice::<ClientMessage>(&data) {
                            Ok(client_msg) => {
                                let mut state = shared_state.lock().unwrap();
                                state
                                    .pending_messages
                                    .push_back((connection_id, client_msg));
                            }
                            Err(e) => {
                                eprintln!("Failed to deserialize ClientMessage: {e}");
                            }
                        }
                    }
                    Ok(Message::Close(_)) => {
                        // Remove connection
                        let mut state = shared_state.lock().unwrap();
                        state.connections.remove(&connection_id);
                        break;
                    }
                    Ok(Message::Ping(_data)) => {
                        // Auto-respond to pings via the channel
                        // The send task will handle sending the pong
                        // For now, we'll just ignore pings (tungstenite handles them automatically)
                        // If we need custom ping handling, we'd need to send through the channel
                    }
                    Ok(Message::Pong(_)) => {
                        // Ignore pongs
                    }
                    Ok(Message::Frame(_)) => {
                        // Ignore raw frames
                    }
                    Err(e) => {
                        eprintln!("WebSocket error: {e}");
                        let mut state = shared_state.lock().unwrap();
                        state.connections.remove(&connection_id);
                        break;
                    }
                }
            }

            // Cleanup: remove connection on exit
            let mut state = shared_state.lock().unwrap();
            state.connections.remove(&connection_id);
        });
    }
}

impl ServerTransport for WebSocketServerTransport {
    fn send(&mut self, msg: ServerMessage) -> Result<(), TransportError> {
        // Send to the first available connection
        // TODO: In phase 7, we'll need to route messages to the correct connection
        // based on the request ID or connection tracking
        // For now, serialize the message and send to first connection
        // (we can't clone ServerMessage, so we serialize/deserialize for each connection)

        let json = serde_json::to_string(&msg).map_err(|e| {
            TransportError::Serialization(format!("Failed to serialize ServerMessage: {e}"))
        })?;

        let state = self.shared_state.lock().unwrap();

        if state.connections.is_empty() {
            return Err(TransportError::Other(
                "No connected clients to send message to".to_string(),
            ));
        }

        // Find first available connection and send
        // Deserialize for each connection (inefficient but works)
        let mut connection_id_to_remove = None;
        for (connection_id, connection) in state.connections.iter() {
            // Deserialize the message for this connection
            let msg_clone: ServerMessage = match serde_json::from_str(&json) {
                Ok(m) => m,
                Err(e) => {
                    return Err(TransportError::Deserialization(format!(
                        "Failed to deserialize ServerMessage: {e}"
                    )));
                }
            };

            if connection.sender.send(msg_clone).is_err() {
                connection_id_to_remove = Some(*connection_id);
            } else {
                // Successfully sent, done
                return Ok(());
            }
        }

        // All connections failed, remove the failed one
        drop(state);
        if let Some(id) = connection_id_to_remove {
            let mut state = self.shared_state.lock().unwrap();
            state.connections.remove(&id);
        }

        Err(TransportError::Other(
            "Failed to send message to any connected client".to_string(),
        ))
    }

    fn receive(&mut self) -> Result<Option<ClientMessage>, TransportError> {
        // Check for messages in the queue
        let mut state = self.shared_state.lock().unwrap();
        Ok(state.pending_messages.pop_front().map(|(_, msg)| msg))
    }

    fn receive_all(&mut self) -> Result<Vec<ClientMessage>, TransportError> {
        // Drain all messages from the queue
        let mut state = self.shared_state.lock().unwrap();
        let mut messages = Vec::new();
        while let Some((_, msg)) = state.pending_messages.pop_front() {
            messages.push(msg);
        }
        Ok(messages)
    }

    fn close(&mut self) -> Result<(), TransportError> {
        // Close all connections by clearing them
        let mut state = self.shared_state.lock().unwrap();
        state.connections.clear();
        state.pending_messages.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    //use super::*;

    #[test]
    fn test_websocket_server_transport_creation() {
        // This test would require binding to a port
        // For now, just verify the struct can be created conceptually
        // Actual connection tests will be in integration tests
    }
}
