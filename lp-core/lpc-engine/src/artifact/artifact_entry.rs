//! Single artifact slot in [`super::ArtifactManager`].

use lpc_model::FrameId;
use lpc_source::SrcArtifactSpec;

use super::{ArtifactError, ArtifactState};

/// One artifact record: authored spec, refcount, last successful content frame, and state.
pub struct ArtifactEntry<A> {
    pub spec: SrcArtifactSpec,
    pub state: ArtifactState<A>,
    pub refcount: u32,
    pub content_frame: FrameId,
    /// Secondary slot for errors not represented in [`ArtifactState`] (currently unused; kept for API parity).
    pub error: Option<ArtifactError>,
}
