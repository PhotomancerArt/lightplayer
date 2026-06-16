//! Overlay commit failures.

use alloc::string::String;

use lpc_model::ArtifactLocation;

/// Error while committing pending overlay edits to durable artifacts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommitError {
    Filesystem {
        location: ArtifactLocation,
        message: String,
    },
    Projection {
        location: ArtifactLocation,
        message: String,
    },
}
