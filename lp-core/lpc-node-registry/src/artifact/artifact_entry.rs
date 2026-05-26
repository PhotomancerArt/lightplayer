//! Single artifact record in [`super::ArtifactStore`].

use lpc_model::Revision;

use super::{ArtifactId, ArtifactLocation, ArtifactReadState};

/// One project file artifact: stable id, path, content revision, read outcome.
pub struct ArtifactEntry {
    pub id: ArtifactId,
    pub location: ArtifactLocation,
    pub revision: Revision,
    pub read_state: ArtifactReadState,
}
