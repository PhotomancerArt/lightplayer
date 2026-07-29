//! A fixed-buffer bump allocator (never frees) so fixtures can use `alloc`
//! collections. The guest is single-threaded by construction (one emulated
//! core, no interrupts), so a plain non-atomic cursor is sound — and avoids
//! `s32c1i` compare-and-swap sequences the emulator does not implement.

use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;

const HEAP_SIZE: usize = 16 * 1024;

struct BumpAllocator {
    heap: UnsafeCell<[u8; HEAP_SIZE]>,
    next: UnsafeCell<usize>,
}

// SAFETY: the emulated guest is single-threaded (no interrupts, one core), so
// no concurrent access to the cursor or heap is possible.
unsafe impl Sync for BumpAllocator {}

unsafe impl GlobalAlloc for BumpAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: single-threaded guest; no other reference to `next` or the
        // heap buffer is live during this call.
        unsafe {
            let next = &mut *self.next.get();
            let base = self.heap.get() as *mut u8;
            let start = (*next + layout.align() - 1) & !(layout.align() - 1);
            let end = match start.checked_add(layout.size()) {
                Some(e) if e <= HEAP_SIZE => e,
                _ => return core::ptr::null_mut(),
            };
            *next = end;
            base.add(start)
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // Bump allocator: never frees.
    }
}

#[global_allocator]
static ALLOCATOR: BumpAllocator = BumpAllocator {
    heap: UnsafeCell::new([0u8; HEAP_SIZE]),
    next: UnsafeCell::new(0),
};
