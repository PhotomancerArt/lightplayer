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
    /// The device refused the request because this link does not hold the
    /// tier it needs (`NotPermitted { needs }`) — a Bluetooth link logged in
    /// at play, asked for an edit. Always a reply, never a timeout, so a
    /// caller can say "this needs an edit password" instead of "failed".
    NotPermitted { needs: lpc_access::Tier },
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
            // The sentence a person reads: every caller that shows an
            // error (a push outcome, an action log) says what to do.
            Self::NotPermitted { needs } => f.write_str(match needs {
                lpc_access::Tier::Edit => "This needs an edit password — log in again with one.",
                lpc_access::Tier::Play => "This needs a password — log in first.",
            }),
        }
    }
}

impl Error for ClientError {}

impl From<TransportError> for ClientError {
    fn from(error: TransportError) -> Self {
        Self::Transport(error.to_string())
    }
}
