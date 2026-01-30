//! Transport error type
//!
//! Moved from lp-shared to lp-model to break circular dependency.
//! Transport errors are related to message protocol, so they belong in lp-model.

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
