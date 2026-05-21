//! Structured errors for artifact store operations.

use alloc::string::String;

use super::ArtifactReadFailure;

/// Errors returned by [`super::ArtifactStore`] and read operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactError {
    /// No entry exists for this [`super::ArtifactId`] handle.
    UnknownHandle { handle: u32 },
    /// [`super::ArtifactStore::release`] called when refcount is already zero.
    InvalidRelease { handle: u32 },
    /// Locator resolution failed at acquire time (no entry created).
    Resolution(String),
    /// Transient read failed; see [`ArtifactReadFailure`] on the entry.
    Read(ArtifactReadFailure),
    /// Internal invariant violation (should not happen for file artifacts).
    Internal(String),
}

impl ArtifactError {
    pub(crate) fn internal(message: impl Into<String>) -> Self {
        Self::Internal(message.into())
    }
}
