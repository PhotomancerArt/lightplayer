//! Errors from parsing or resolving artifact locations.

use alloc::string::String;

/// Location parse/resolution failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactLocationError {
    Resolution(String),
}
