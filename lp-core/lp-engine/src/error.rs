use alloc::format;
use alloc::string::String;
use lp_model::NodeKind;

/// Engine error type
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// I/O error
    Io {
        /// Path that failed
        path: String,
        /// Error details
        details: String,
    },
    /// Parse error (JSON, etc.)
    Parse {
        /// File being parsed
        file: String,
        /// Parse error details
        error: String,
    },
    /// Not found
    NotFound {
        /// Path that was not found
        path: String,
    },
    /// Invalid configuration
    InvalidConfig {
        /// Node path
        node_path: String,
        /// Reason for invalidity
        reason: String,
    },
    /// Wrong node kind (for resolution)
    WrongNodeKind {
        /// Node specifier that was resolved
        specifier: String,
        /// Expected node kind
        expected: NodeKind,
        /// Actual node kind
        actual: NodeKind,
    },
    /// Other error
    Other {
        /// Error message
        message: String,
    },
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::Io { path, details } => {
                write!(f, "I/O error: {details} ({path})")
            }
            Error::Parse { file, error } => {
                write!(f, "Parse error in {file}: {error}")
            }
            Error::NotFound { path } => {
                write!(f, "Not found: {path}")
            }
            Error::InvalidConfig { node_path, reason } => {
                write!(f, "Invalid config for {node_path}: {reason}")
            }
            Error::WrongNodeKind {
                specifier,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "Wrong node kind for {specifier}: expected {expected:?}, got {actual:?}"
                )
            }
            Error::Other { message } => {
                write!(f, "Error: {message}")
            }
        }
    }
}

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "std")]
impl std::error::Error for Error {}

impl From<lp_shared::OutputError> for Error {
    fn from(err: lp_shared::OutputError) -> Self {
        match err {
            lp_shared::OutputError::PinAlreadyOpen { pin } => Error::Other {
                message: format!("Pin {pin} is already open"),
            },
            lp_shared::OutputError::InvalidHandle { handle } => Error::Other {
                message: format!("Invalid handle: {handle}"),
            },
            lp_shared::OutputError::InvalidConfig { reason } => Error::InvalidConfig {
                node_path: String::from("output"),
                reason,
            },
            lp_shared::OutputError::DataLengthMismatch { expected, actual } => {
                Error::InvalidConfig {
                    node_path: String::from("output"),
                    reason: format!(
                        "Data length {actual} doesn't match expected byte_count {expected}"
                    ),
                }
            }
            lp_shared::OutputError::Other { message } => Error::Other { message },
        }
    }
}
