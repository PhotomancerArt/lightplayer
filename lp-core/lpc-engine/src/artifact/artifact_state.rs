//! Lifecycle state for a resolved artifact entry.

use lpc_model::NodeDef;

/// State of an artifact entry in the runtime cache.
#[derive(Debug)]
pub enum ArtifactState {
    /// Spec is known and refcounted; payload has not been loaded yet.
    Resolved,
    /// Payload loaded successfully.
    Loaded(NodeDef),
    /// Payload prepared for use (reserved for future prepare hooks).
    Prepared(NodeDef),
    /// No active refs; payload retained until eviction or reload.
    Idle(NodeDef),
    ResolutionError(alloc::string::String),
    LoadError(alloc::string::String),
    PrepareError(alloc::string::String),
}
