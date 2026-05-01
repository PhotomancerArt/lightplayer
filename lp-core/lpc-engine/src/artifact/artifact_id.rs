//! Opaque handle to an artifact entry inside [`super::ArtifactManager`].

/// Runtime handle returned by [`super::ArtifactManager::acquire_resolved`].
///
/// Dropping a reference does **not** decrement refcount; call [`super::ArtifactManager::release`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
