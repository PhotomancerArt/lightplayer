//! Client-side transport trait
//!
//! Defines the async interface for client-side transport implementations.
//! Messages are consumed (moved) on send, and receive operations are async.

use lp_model::{ClientMessage, ServerMessage, TransportError};

/// Trait for client-side transport implementations
///
/// This trait provides an async interface for sending and receiving messages.
/// Messages are consumed (moved) on send. Receive operations are async and will
/// yield to the async runtime when waiting for data.
///
/// The transport handles serialization/deserialization internally.
///
/// # Examples
///
/// ```rust,no_run
/// use lp_cli::client::transport::ClientTransport;
/// use lp_model::{ClientMessage, ServerMessage, TransportError};
///
/// struct MyTransport;
///
/// #[async_trait::async_trait]
/// impl ClientTransport for MyTransport {
///     async fn send(&mut self, msg: ClientMessage) -> Result<(), TransportError> {
///         // Send message (transport handles serialization)
///         Ok(())
///     }
///
///     async fn receive(&mut self) -> Result<ServerMessage, TransportError> {
///         // Receive message (transport handles deserialization)
///         // This will wait until a message is available
///         # Err(TransportError::ConnectionLost)
///     }
///
///     async fn close(&mut self) -> Result<(), TransportError> {
///         // Close the transport
///         Ok(())
///     }
/// }
/// ```
#[async_trait::async_trait]
pub trait ClientTransport: Send {
    /// Send a client message (consumes the message)
    ///
    /// The transport handles serialization internally. This operation may
    /// involve network I/O or other async operations.
    ///
    /// # Arguments
    ///
    /// * `msg` - The client message to send (consumed/moved)
    ///
    /// # Returns
    ///
    /// * `Ok(())` if the message was sent successfully
    /// * `Err(TransportError)` if sending failed
    async fn send(&mut self, msg: ClientMessage) -> Result<(), TransportError>;

    /// Receive a server message (async, blocking)
    ///
    /// The transport handles deserialization internally. This operation will
    /// wait until a message is available, yielding to the async runtime while
    /// waiting. This is the primary method for receiving messages.
    ///
    /// # Returns
    ///
    /// * `Ok(ServerMessage)` if a message was received
    /// * `Err(TransportError::ConnectionLost)` if the transport is closed
    /// * `Err(TransportError)` for other receiving errors
    async fn receive(&mut self) -> Result<ServerMessage, TransportError>;

    /// Close the transport connection
    ///
    /// Explicitly closes the transport connection. This method is idempotent -
    /// calling it multiple times is safe and will return `Ok(())` if already closed.
    /// This operation may involve async cleanup (e.g., sending close frames).
    ///
    /// # Returns
    ///
    /// * `Ok(())` if the transport was closed successfully (or already closed)
    /// * `Err(TransportError)` if closing failed
    #[allow(
        dead_code,
        reason = "Required trait method, will be used in cleanup scenarios"
    )]
    async fn close(&mut self) -> Result<(), TransportError>;
}
