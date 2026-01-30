//! Global allocator setup for emulator guest code
//!
//! Initializes a global allocator using the heap section defined in the linker script.

use linked_list_allocator::LockedHeap;

/// Heap allocator instance
///
/// This will be initialized by `init_heap()` using the heap section
/// from the linker script (__heap_start to __heap_end).
#[global_allocator]
static HEAP_ALLOCATOR: LockedHeap = LockedHeap::empty();

/// Initialize the global heap allocator
///
/// This function must be called before any heap allocations are made.
/// It sets up the allocator to use the heap section defined in the linker script.
///
/// # Safety
///
/// This function is unsafe because it:
/// - Accesses linker script symbols directly
/// - Initializes the global allocator (must only be called once)
pub unsafe fn init_heap() {
    unsafe extern "C" {
        static __heap_start: u8;
        static __heap_end: u8;
    }

    let heap_start_addr = core::ptr::addr_of!(__heap_start) as usize;
    let heap_end_addr = core::ptr::addr_of!(__heap_end) as usize;
    let heap_size = heap_end_addr - heap_start_addr;
    let heap_start = heap_start_addr as *mut u8;

    unsafe {
        HEAP_ALLOCATOR.lock().init(heap_start, heap_size);
    }
}
