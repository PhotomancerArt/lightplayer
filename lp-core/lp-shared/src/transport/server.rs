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
///     async fn send(&mut self, msg: ServerMessage) -> Result<(), TransportError> {
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
    async fn send(&mut self, msg: ServerMessage) -> Result<(), TransportError>;

    /// Receive a client message (non-blocking). Returns `Ok(None)` if no message is available.
    async fn receive(&mut self) -> Result<Option<ClientMessage>, TransportError>;

    /// Receive all available client messages (non-blocking)
    async fn receive_all(&mut self) -> Result<Vec<ClientMessage>, TransportError>;

    /// Close the transport connection
    async fn close(&mut self) -> Result<(), TransportError>;
}
