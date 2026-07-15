//! Opaque RAII handle to a backend texture allocation.

use alloc::sync::Arc;
use core::any::Any;

use lps_shared::TextureStorageFormat;

use crate::handle_allocator::{HandleAllocator, HandleBacking};

/// Opaque handle to a texture owned by an [`crate::LpGraphics`] backend.
///
/// RAII: dropping the handle returns the allocation to the backend that
/// created it. There is no texel access here — bytes move through
/// [`crate::LpGraphics::read_back`] / [`crate::LpGraphics::write_texture`].
/// A handle is only valid with the backend that created it.
pub struct TextureHandle {
    width: u32,
    height: u32,
    format: TextureStorageFormat,
    /// `Some` until `Drop`; taken exactly once.
    backing: Option<HandleBacking>,
    allocator: Arc<dyn HandleAllocator>,
}

impl TextureHandle {
    /// Assemble a handle around a backend allocation. **Backend-facing**:
    /// only [`crate::LpGraphics`] implementations construct handles.
    pub fn from_backend_parts(
        width: u32,
        height: u32,
        format: TextureStorageFormat,
        backing: HandleBacking,
        allocator: Arc<dyn HandleAllocator>,
    ) -> Self {
        Self {
            width,
            height,
            format,
            backing: Some(backing),
            allocator,
        }
    }

    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    #[must_use]
    pub fn format(&self) -> TextureStorageFormat {
        self.format
    }

    /// Backend allocation behind this handle. **Backend-facing**: consumers
    /// never inspect the backing; backends downcast it to their own type.
    #[must_use]
    pub fn backing(&self) -> &(dyn Any + Send + Sync) {
        self.backing
            .as_deref()
            .expect("texture handle backing is present until drop")
    }

    /// Mutable backend allocation behind this handle. **Backend-facing.**
    #[must_use]
    pub fn backing_mut(&mut self) -> &mut (dyn Any + Send + Sync) {
        self.backing
            .as_deref_mut()
            .expect("texture handle backing is present until drop")
    }
}

impl Drop for TextureHandle {
    fn drop(&mut self) {
        if let Some(backing) = self.backing.take() {
            self.allocator.free_texture(backing);
        }
    }
}

impl core::fmt::Debug for TextureHandle {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TextureHandle")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("format", &self.format)
            .finish_non_exhaustive()
    }
}
