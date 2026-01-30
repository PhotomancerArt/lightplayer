//! Client-side transport trait
//!
//! Defines the interface for client-side transport implementations.
//! Messages are consumed (moved) on send, and receive operations are async.
//!
//! The transport handles serialization/deserialization internally.
//!
//! This trait is async-first, designed for efficient I/O operations without
//! requiring busy-waiting or polling wrappers.

extern crate alloc;

use alloc::{boxed::Box, vec::Vec};
use async_trait::async_trait;
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
/// use async_trait::async_trait;
/// use lp_client::transport::ClientTransport;
/// use lp_model::{ClientMessage, ServerMessage, TransportError};
///
/// struct MyTransport;
///
/// #[async_trait]
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
/// }
/// ```
#[async_trait]
pub trait ClientTransport {
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

    /// Try to receive all currently buffered messages (non-blocking)
    ///
    /// Drains all messages that are currently available in the transport's
    /// internal buffer without waiting for more. This is useful for batch
    /// processing when you know messages may have queued up.
    ///
    /// This method will not block or wait - it only returns messages that are
    /// immediately available. If no messages are buffered, returns an empty vector.
    ///
    /// **Note**: The default implementation returns an empty vector. Transport
    /// implementations should override this method to provide efficient buffered
    /// message draining.
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<ServerMessage>)` - Vector of all currently buffered messages (may be empty)
    /// * `Err(TransportError)` if receiving failed or transport is closed
    async fn try_receive_all(&mut self) -> Result<Vec<ServerMessage>, TransportError>;

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
    async fn close(&mut self) -> Result<(), TransportError>;
}
