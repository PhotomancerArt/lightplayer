//! Structured errors for artifact store operations.

use alloc::string::String;

use super::ArtifactLocation;
use super::ArtifactReadFailure;

/// Errors returned by [`super::ArtifactStore`] and read operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactError {
    /// No catalog entry exists for this [`ArtifactLocation`].
    UnknownArtifact { location: ArtifactLocation },
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
