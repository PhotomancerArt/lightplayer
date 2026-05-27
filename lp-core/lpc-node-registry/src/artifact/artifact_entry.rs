//! Single artifact record in [`super::ArtifactStore`].

use lpc_model::Revision;

use super::{ArtifactLoc, ArtifactReadState};

/// One registered project artifact: location, content revision, read outcome.
pub struct ArtifactEntry {
    pub location: ArtifactLoc,
    pub revision: Revision,
    pub read_state: ArtifactReadState,
}
