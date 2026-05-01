//! Structured errors for artifact manager operations and loader callbacks.

use alloc::string::String;

/// Errors returned by [`super::ArtifactManager`] and loader closures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactError {
    /// No entry exists for this [`super::ArtifactId`] handle.
    UnknownHandle { handle: u32 },
    /// [`super::ArtifactManager::release`] called when refcount is already zero.
    InvalidRelease { handle: u32 },
    /// Artifact resolution failed (forwarded into [`super::ArtifactState::ResolutionError`] when stored).
    Resolution(String),
    /// Load failed (forwarded into [`super::ArtifactState::LoadError`] when stored).
    Load(String),
    /// Prepare failed (forwarded into [`super::ArtifactState::PrepareError`] when stored).
    Prepare(String),
}

impl ArtifactError {
    pub(crate) fn summary_for_state(&self) -> String {
        match self {
            ArtifactError::UnknownHandle { handle } => {
                alloc::format!("unknown artifact handle {handle}")
            }
            ArtifactError::InvalidRelease { handle } => {
                alloc::format!("invalid release for handle {handle}")
            }
            ArtifactError::Resolution(s) | ArtifactError::Load(s) | ArtifactError::Prepare(s) => {
                s.clone()
            }
        }
    }
}
