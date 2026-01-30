//! Server-side transport trait
//!
//! Defines the interface for server-side transport implementations.
//! Messages are consumed (moved) on send, and receive is non-blocking.
//!
//! The transport handles serialization/deserialization internally.

extern crate alloc;

use alloc::vec::Vec;
use lp_model::{ClientMessage, ServerMessage, TransportError};

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
/// use lp_shared::transport::ServerTransport;
/// use lp_model::{ClientMessage, ServerMessage, TransportError};
///
/// struct MyTransport;
///
/// impl ServerTransport for MyTransport {
///     fn send(&mut self, msg: ServerMessage) -> Result<(), TransportError> {
///         // Send message (transport handles serialization)
///         Ok(())
///     }
///
///     fn receive(&mut self) -> Result<Option<ClientMessage>, TransportError> {
///         // Receive message (transport handles deserialization)
///         Ok(None)
///     }
///
///     fn close(&mut self) -> Result<(), TransportError> {
///         // Close the transport connection
///         Ok(())
///     }
/// }
/// ```
pub trait ServerTransport {
    /// Send a server message (consumes the message)
    ///
    /// The transport handles serialization internally.
    ///
    /// # Arguments
    ///
    /// * `msg` - The server message to send (consumed/moved)
    ///
    /// # Returns
    ///
    /// * `Ok(())` if the message was sent successfully
    /// * `Err(TransportError)` if sending failed
    fn send(&mut self, msg: ServerMessage) -> Result<(), TransportError>;

    /// Receive a client message (non-blocking)
    ///
    /// The transport handles deserialization internally.
    ///
    /// # Returns
    ///
    /// * `Ok(Some(ClientMessage))` if a message is available
    /// * `Ok(None)` if no message is available (non-blocking)
    /// * `Err(TransportError)` if receiving failed
    fn receive(&mut self) -> Result<Option<ClientMessage>, TransportError>;

    /// Receive all available client messages (non-blocking)
    ///
    /// Drains all available messages from the transport in a single call.
    /// This is more efficient than calling `receive()` in a loop.
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<ClientMessage>)` - Vector of all available messages (may be empty)
    /// * `Err(TransportError)` if receiving failed
    fn receive_all(&mut self) -> Result<Vec<ClientMessage>, TransportError> {
        let mut messages = Vec::new();
        loop {
            match self.receive()? {
                Some(msg) => messages.push(msg),
                None => break,
            }
        }
        Ok(messages)
    }

    /// Close the transport connection
    ///
    /// Explicitly closes the transport connection. This method is idempotent -
    /// calling it multiple times is safe and will return `Ok(())` if already closed.
    ///
    /// # Returns
    ///
    /// * `Ok(())` if the transport was closed successfully (or already closed)
    /// * `Err(TransportError)` if closing failed
    fn close(&mut self) -> Result<(), TransportError>;
}
