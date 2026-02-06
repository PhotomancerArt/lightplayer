//! WebSocket client transport
//!
//! Implements `ClientTransport` using async `tokio-tungstenite`.

use crate::transport::ClientTransport;
use futures_util::{SinkExt, StreamExt};
use lp_model::{ClientMessage, ServerMessage, TransportError};
use tokio::net::TcpStream;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

/// WebSocket client transport
///
/// Uses async `tokio-tungstenite` for WebSocket communication.
pub struct WebSocketClientTransport {
    /// WebSocket stream (None if disconnected)
    stream: Option<WebSocketStream<MaybeTlsStream<TcpStream>>>,
    /// Whether the transport is closed
    closed: bool,
}

impl WebSocketClientTransport {
    /// Create a new WebSocket client transport and connect to the server
    ///
    /// # Arguments
    ///
    /// * `url` - WebSocket URL (e.g., `ws://localhost:2812/`)
    ///
    /// # Returns
    ///
    /// * `Ok(Self)` if connection succeeded
    /// * `Err(TransportError)` if connection failed
    pub async fn new(url: &str) -> Result<Self, TransportError> {
        // Connect via tokio-tungstenite
        let (stream, _) = connect_async(url).await.map_err(|e| {
            TransportError::Other(format!(
                "Failed to establish WebSocket connection to '{url}': {e}"
            ))
        })?;

        Ok(Self {
            stream: Some(stream),
            closed: false,
        })
    }
}

#[async_trait::async_trait]
impl ClientTransport for WebSocketClientTransport {
    async fn send(&mut self, msg: ClientMessage) -> Result<(), TransportError> {
        if self.closed {
            return Err(TransportError::ConnectionLost);
        }

        let stream = match &mut self.stream {
            Some(s) => s,
            None => return Err(TransportError::ConnectionLost),
        };

        // Serialize ClientMessage to JSON
        let json = lp_model::json::to_string(&msg).map_err(|e| {
            TransportError::Serialization(format!("Failed to serialize ClientMessage: {e}"))
        })?;

        // Send as text message
        stream
            .send(tokio_tungstenite::tungstenite::Message::Text(json))
            .await
            .map_err(|e| TransportError::Other(format!("Failed to send message: {e}")))?;

        Ok(())
    }

    async fn receive(&mut self) -> Result<ServerMessage, TransportError> {
        if self.closed {
            return Err(TransportError::ConnectionLost);
        }

        let stream = match &mut self.stream {
            Some(s) => s,
            None => return Err(TransportError::ConnectionLost),
        };

        // Wait for next message from stream
        loop {
            match stream.next().await {
                Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text))) => {
                    // Deserialize ServerMessage from JSON
                    return lp_model::json::from_str(&text).map_err(|e| {
                        TransportError::Deserialization(format!(
                            "Failed to deserialize ServerMessage: {e}"
                        ))
                    });
                }
                Some(Ok(tokio_tungstenite::tungstenite::Message::Binary(data))) => {
                    // Deserialize ServerMessage from binary JSON
                    return lp_model::json::from_slice(&data).map_err(|e| {
                        TransportError::Deserialization(format!(
                            "Failed to deserialize ServerMessage: {e}"
                        ))
                    });
                }
                Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_))) => {
                    self.stream = None;
                    return Err(TransportError::ConnectionLost);
                }
                Some(Ok(tokio_tungstenite::tungstenite::Message::Ping(_))) => {
                    // Auto-respond to pings (tokio-tungstenite handles this automatically)
                    continue;
                }
                Some(Ok(tokio_tungstenite::tungstenite::Message::Pong(_))) => {
                    // Ignore pongs
                    continue;
                }
                Some(Ok(tokio_tungstenite::tungstenite::Message::Frame(_))) => {
                    // Ignore raw frames
                    continue;
                }
                Some(Err(e)) => {
                    // WebSocket error
                    self.stream = None;
                    return Err(TransportError::Other(format!("WebSocket error: {e}")));
                }
                None => {
                    // Stream ended
                    self.stream = None;
                    return Err(TransportError::ConnectionLost);
                }
            }
        }
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        if self.closed {
            return Ok(());
        }

        self.closed = true;

        // Send close frame if stream is still open
        if let Some(stream) = &mut self.stream {
            let _ = stream.close(None).await;
        }

        // Clear stream
        self.stream = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_model::{AsLpPathBuf, server::FsRequest};

    #[test]
    fn test_serialization_format() {
        // Test that we serialize/deserialize correctly
        let msg = ClientMessage {
            id: 1,
            msg: lp_model::ClientRequest::Filesystem(FsRequest::Read {
                path: "/test".as_path_buf(),
            }),
        };

        let json = lp_model::json::to_string(&msg).unwrap();
        let deserialized: ClientMessage = lp_model::json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, msg.id);
    }
}
