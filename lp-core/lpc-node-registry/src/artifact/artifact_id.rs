//! Opaque handle to an artifact entry inside [`super::ArtifactStore`].

/// Runtime handle returned by [`super::ArtifactStore::acquire_location`].
///
/// Dropping a caller's interest does **not** decrement refcount; call
/// [`super::ArtifactStore::release`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ArtifactId {
    handle: u32,
}

impl ArtifactId {
    pub(crate) const fn from_raw(handle: u32) -> Self {
        Self { handle }
    }

    pub fn handle(&self) -> u32 {
        self.handle
    }
}
