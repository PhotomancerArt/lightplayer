//! Portable errors for the LightPlayer server client.
//!
//! The core client avoids `anyhow` so browser and other non-Tokio runtimes can
//! preserve structured protocol failures. Host adapters may convert these into
//! application-local error types at their boundary.

use std::error::Error;
use std::fmt;

use lpc_wire::TransportError;

pub type ClientResult<T> = Result<T, ClientError>;

/// Error surfaced by the runtime-neutral `LpClient` core.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ClientError {
    /// The underlying I/O channel failed.
    Transport(String),
    /// The server returned an explicit protocol error response.
    Server(String),
    /// The received stream violated the expected client protocol.
    Protocol(String),
    /// A valid response arrived, but not the one required for the operation.
    UnexpectedResponse {
        operation: &'static str,
        response: String,
    },
}

impl ClientError {
    pub fn unexpected_response(operation: &'static str, response: impl fmt::Debug) -> Self {
        Self::UnexpectedResponse {
            operation,
            response: format!("{response:?}"),
        }
    }
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport(message) => write!(f, "transport error: {message}"),
            Self::Server(message) => write!(f, "server error: {message}"),
            Self::Protocol(message) => write!(f, "protocol error: {message}"),
            Self::UnexpectedResponse {
                operation,
                response,
            } => write!(f, "unexpected response for {operation}: {response}"),
        }
    }
}

impl Error for ClientError {}

impl From<TransportError> for ClientError {
    fn from(error: TransportError) -> Self {
        Self::Transport(error.to_string())
    }
}
