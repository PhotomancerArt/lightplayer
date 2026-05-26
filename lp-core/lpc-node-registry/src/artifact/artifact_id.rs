//! Opaque id for an artifact entry inside [`super::ArtifactStore`].

/// Runtime id returned by [`super::ArtifactStore::register_file`].
///
/// Remains valid while the artifact is registered in the store catalog.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct ArtifactId {
    id: u32,
}

impl ArtifactId {
    pub(crate) const fn from_raw(id: u32) -> Self {
        Self { id }
    }

    pub const fn raw(self) -> u32 {
        self.id
    }
}
