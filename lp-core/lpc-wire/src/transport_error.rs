//! Transport error type
//!
//! Transport errors are protocol-adjacent: shared transports use this type to
//! report framing, serialization, and connection failures without depending on
//! app or firmware-specific error stacks.

use alloc::string::String;
use core::fmt;

/// Transport error type
#[derive(Debug, Clone)]
pub enum TransportError {
    /// Serialization error
    Serialization(String),
    /// Deserialization error
    Deserialization(String),
    /// Connection lost
    ConnectionLost,
    /// Other transport error
    Other(String),
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransportError::Serialization(msg) => write!(f, "Serialization error: {msg}"),
            TransportError::Deserialization(msg) => {
                write!(f, "Deserialization error: {msg}")
            }
            TransportError::ConnectionLost => write!(f, "Connection lost"),
            TransportError::Other(msg) => write!(f, "Transport error: {msg}"),
        }
    }
}
