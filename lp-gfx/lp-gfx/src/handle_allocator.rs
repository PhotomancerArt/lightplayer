//! Backend deallocation hook carried by every resource handle.

use alloc::boxed::Box;
use core::any::Any;

/// Type-erased backend allocation owned by a handle.
///
/// Only the backend that produced a handle knows the concrete type behind
/// this box (e.g. `lp-gfx-lpvm` stores `LpsTextureBuf`); consumers never
/// inspect it.
pub type HandleBacking = Box<dyn Any + Send + Sync>;

/// Backend-facing deallocation vtable wired into every handle.
///
/// Handles hold an `Arc<dyn HandleAllocator>` to the backend that allocated
/// them; `Drop` routes the [`HandleBacking`] back through these methods. The
/// `Arc` also keeps the backing memory pool alive for as long as any handle
/// exists. Engine code never calls this trait directly.
pub trait HandleAllocator: Send + Sync {
    /// Return a texture allocation ([`crate::TextureHandle`] backing).
    fn free_texture(&self, backing: HandleBacking);

    /// Return a sample-point buffer ([`crate::SamplePointsHandle`] backing).
    fn free_sample_points(&self, backing: HandleBacking);

    /// Return a sample-output buffer ([`crate::SampleOutHandle`] backing).
    fn free_sample_out(&self, backing: HandleBacking);
}
