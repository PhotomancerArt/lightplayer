//! Opaque RAII handle to a backend sample-point buffer.

use alloc::sync::Arc;
use core::any::Any;

use crate::handle_allocator::{HandleAllocator, HandleBacking};

/// Opaque handle to a buffer of Q16.16 shader pixel-space sample points
/// (`[x_q16, y_q16]` pairs) owned by an [`crate::LpGraphics`] backend.
///
/// RAII: dropping the handle returns the allocation. Point data moves through
/// [`crate::LpGraphics::write_sample_points`] /
/// [`crate::LpGraphics::read_sample_points`].
pub struct SamplePointsHandle {
    count: u32,
    /// `Some` until `Drop`; taken exactly once.
    backing: Option<HandleBacking>,
    allocator: Arc<dyn HandleAllocator>,
}

impl SamplePointsHandle {
    /// Assemble a handle around a backend allocation. **Backend-facing.**
    pub fn from_backend_parts(
        count: u32,
        backing: HandleBacking,
        allocator: Arc<dyn HandleAllocator>,
    ) -> Self {
        Self {
            count,
            backing: Some(backing),
            allocator,
        }
    }

    /// Number of sample points (each point is two `i32` Q16.16 coordinates).
    #[must_use]
    pub fn count(&self) -> u32 {
        self.count
    }

    /// Backend allocation behind this handle. **Backend-facing.**
    #[must_use]
    pub fn backing(&self) -> &(dyn Any + Send + Sync) {
        self.backing
            .as_deref()
            .expect("sample point handle backing is present until drop")
    }

    /// Mutable backend allocation behind this handle. **Backend-facing.**
    #[must_use]
    pub fn backing_mut(&mut self) -> &mut (dyn Any + Send + Sync) {
        self.backing
            .as_deref_mut()
            .expect("sample point handle backing is present until drop")
    }
}

impl Drop for SamplePointsHandle {
    fn drop(&mut self) {
        if let Some(backing) = self.backing.take() {
            self.allocator.free_sample_points(backing);
        }
    }
}

impl core::fmt::Debug for SamplePointsHandle {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SamplePointsHandle")
            .field("count", &self.count)
            .finish_non_exhaustive()
    }
}
