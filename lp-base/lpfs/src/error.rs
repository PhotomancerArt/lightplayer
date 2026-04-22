//! Error types for lpfs

use alloc::string::String;
use core::fmt;

/// Filesystem error type
#[derive(Debug, Clone)]
pub enum FsError {
    /// Filesystem operation failed
    Filesystem(String),
    /// File not found
    NotFound(String),
    /// Invalid path
    InvalidPath(String),
}

impl fmt::Display for FsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FsError::Filesystem(msg) => write!(f, "Filesystem error: {msg}"),
            FsError::NotFound(msg) => write!(f, "File not found: {msg}"),
            FsError::InvalidPath(msg) => write!(f, "Invalid path: {msg}"),
        }
    }
}
