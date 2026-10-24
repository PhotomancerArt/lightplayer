//! The firmware's global allocator: `esp_alloc::HEAP`, behind one check.
//!
//! esp-alloc's own `global-allocator` feature is off so that this wrapper can
//! stand in front of it. On a `two-core` image a request from core 1 goes to
//! [`crate::core1::halt_core1`] instead of the heap: core 1 may be stalled by
//! a flash write at any instruction, and a core stalled inside the
//! allocator's lock would deadlock the writer (`core1`'s module docs). Every
//! other request, and every request on an image without `two-core`, is
//! exactly `esp_alloc::HEAP`'s (`alloc`, `dealloc`, and the trait's default
//! `realloc` over them, as before).
//!
//! The check is one `rsr.prid` and a compare per call.

use core::alloc::{GlobalAlloc, Layout};

/// The wrapper. See [the module docs](self).
pub struct GuardedHeap;

#[global_allocator]
static ALLOCATOR: GuardedHeap = GuardedHeap;

unsafe impl GlobalAlloc for GuardedHeap {
    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        refuse_core1();
        // SAFETY: the caller's contract, forwarded unchanged.
        unsafe { esp_alloc::HEAP.alloc(layout) }
    }

    #[inline]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        refuse_core1();
        // SAFETY: the caller's contract, forwarded unchanged.
        unsafe { esp_alloc::HEAP.dealloc(ptr, layout) }
    }
}

#[inline(always)]
fn refuse_core1() {
    #[cfg(s3_two_core)]
    if crate::core1::on_core1() {
        crate::core1::halt_core1(crate::core1::Stop::Allocation);
    }
}
