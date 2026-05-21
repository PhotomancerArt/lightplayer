//! Single artifact record in [`super::ArtifactStore`].

use lpc_model::Revision;

use super::{ArtifactId, ArtifactLocation, ArtifactReadState};

/// One held artifact: identity, requester refcount, content revision, read outcome.
pub struct ArtifactEntry {
    pub id: ArtifactId,
    pub location: ArtifactLocation,
    pub refcount: u32,
    pub revision: Revision,
    pub read_state: ArtifactReadState,
}
