//! Error types for lp-server

extern crate alloc;

use alloc::string::String;
use core::fmt;

/// Error type for lp-server operations
#[derive(Debug, Clone)]
pub enum ServerError {
    /// Project not found
    ProjectNotFound(String),
    /// Project already exists
    ProjectExists(String),
    /// Filesystem error
    Filesystem(String),
    /// Core error (from lp-engine)
    Core(String),
    /// Serialization error
    Serialization(String),
}

impl fmt::Display for ServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServerError::ProjectNotFound(name) => {
                write!(f, "Project not found: {name}")
            }
            ServerError::ProjectExists(name) => {
                write!(f, "Project already exists: {name}")
            }
            ServerError::Filesystem(msg) => write!(f, "Filesystem error: {msg}"),
            ServerError::Core(msg) => write!(f, "Core error: {msg}"),
            ServerError::Serialization(msg) => write!(f, "Serialization error: {msg}"),
        }
    }
}

// Note: std::error::Error implementation can be added by users if needed
// We don't implement it here to keep the crate no_std compatible
