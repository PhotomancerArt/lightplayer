//! Opaque RAII handle to a backend RGBA16 sample-output buffer.

use alloc::sync::Arc;
use core::any::Any;

use crate::handle_allocator::{HandleAllocator, HandleBacking};

/// Opaque handle to a buffer of packed RGBA16 sample results owned by an
/// [`crate::LpGraphics`] backend.
///
/// RAII: dropping the handle returns the allocation. Sample data moves
/// through [`crate::LpGraphics::read_sample_out`] /
/// [`crate::LpGraphics::write_sample_out`].
pub struct SampleOutHandle {
    count: u32,
    /// `Some` until `Drop`; taken exactly once.
    backing: Option<HandleBacking>,
    allocator: Arc<dyn HandleAllocator>,
}

impl SampleOutHandle {
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

    /// Number of samples (each sample is four `u16` RGBA channels).
    #[must_use]
    pub fn count(&self) -> u32 {
        self.count
    }

    /// Backend allocation behind this handle. **Backend-facing.**
    #[must_use]
    pub fn backing(&self) -> &(dyn Any + Send + Sync) {
        self.backing
            .as_deref()
            .expect("sample out handle backing is present until drop")
    }

    /// Mutable backend allocation behind this handle. **Backend-facing.**
    #[must_use]
    pub fn backing_mut(&mut self) -> &mut (dyn Any + Send + Sync) {
        self.backing
            .as_deref_mut()
            .expect("sample out handle backing is present until drop")
    }
}

impl Drop for SampleOutHandle {
    fn drop(&mut self) {
        if let Some(backing) = self.backing.take() {
            self.allocator.free_sample_out(backing);
        }
    }
}

impl core::fmt::Debug for SampleOutHandle {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SampleOutHandle")
            .field("count", &self.count)
            .finish_non_exhaustive()
    }
}
