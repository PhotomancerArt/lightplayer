//! Lifecycle state for a resolved artifact entry.

/// State of an artifact entry in the runtime cache.
#[derive(Debug)]
pub enum ArtifactState<A> {
    /// Spec is known and refcounted; payload has not been loaded yet.
    Resolved,
    /// Payload loaded successfully.
    Loaded(A),
    /// Payload prepared for use (reserved for future prepare hooks).
    Prepared(A),
    /// No active refs; payload retained until eviction or reload.
    Idle(A),
    ResolutionError(alloc::string::String),
    LoadError(alloc::string::String),
    PrepareError(alloc::string::String),
}
