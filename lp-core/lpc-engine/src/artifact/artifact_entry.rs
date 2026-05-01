//! Single artifact slot in [`super::ArtifactManager`].

use lpc_model::FrameId;

use super::{ArtifactError, ArtifactId, ArtifactLocation, ArtifactState};

/// One artifact record: runtime id, resolved location, refcount, last successful content frame, and state.
pub struct ArtifactEntry<A> {
    pub id: ArtifactId,
    pub location: ArtifactLocation,
    pub state: ArtifactState<A>,
    pub refcount: u32,
    pub content_frame: FrameId,
    /// Secondary slot for errors not represented in [`ArtifactState`] (currently unused; kept for API parity).
    pub error: Option<ArtifactError>,
}
