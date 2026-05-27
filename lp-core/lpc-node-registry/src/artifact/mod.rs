//! Project artifact catalog: stable locations, freshness metadata, transient reads.

mod artifact_entry;
mod artifact_error;
mod artifact_location;
mod artifact_read_state;
mod artifact_store;

pub use artifact_entry::ArtifactEntry;
pub use artifact_error::ArtifactError;
pub use artifact_location::ArtifactLoc;
pub use artifact_read_state::{ArtifactReadFailure, ArtifactReadState};
pub use artifact_store::ArtifactStore;
