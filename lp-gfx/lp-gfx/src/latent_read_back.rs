//! Caller-owned state for [`crate::LatentReadBackSource::read_back_latent`].

use alloc::boxed::Box;
use core::any::Any;

/// The state a latent readback keeps between calls: whatever a backend
/// needs to answer a read with the most recent frame that has *landed*
/// rather than the one just drawn.
///
/// The caller owns it — one per stable read site (a probe keyed by product
/// and size) — and passes it back on every call. It starts empty; a backend
/// that reads back synchronously never touches it, and one that cannot
/// (the browser GPU tier, which cannot block on a buffer map) fills it on
/// first use with its own staging resources and rebuilds them when the
/// texture's shape changes. Dropping it abandons any read in flight.
#[derive(Default)]
pub struct LatentReadBack {
    backing: Option<Box<dyn Any + Send + Sync>>,
}

impl LatentReadBack {
    /// An empty state: no staging resources, nothing in flight.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The backend's staging state, when one has been installed.
    /// **Backend-facing.**
    #[must_use]
    pub fn backing_mut(&mut self) -> Option<&mut (dyn Any + Send + Sync)> {
        self.backing.as_deref_mut()
    }

    /// Install (or replace) the backend's staging state. **Backend-facing.**
    pub fn set_backing(&mut self, backing: Box<dyn Any + Send + Sync>) {
        self.backing = Some(backing);
    }
}

impl core::fmt::Debug for LatentReadBack {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LatentReadBack")
            .field("staged", &self.backing.is_some())
            .finish()
    }
}
