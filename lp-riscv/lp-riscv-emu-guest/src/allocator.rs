//! Global allocator setup for emulator guest code
//!
//! When the `profile` feature is enabled, wraps the allocator with a
//! `TrackingAllocator` that emits a syscall on every alloc/dealloc/realloc.
//! The host emulator captures these events for offline analysis.

use linked_list_allocator::LockedHeap;

#[cfg(not(feature = "profile"))]
#[global_allocator]
static HEAP_ALLOCATOR: LockedHeap = LockedHeap::empty();

#[cfg(feature = "profile")]
#[global_allocator]
static HEAP_ALLOCATOR: TrackingAllocator = TrackingAllocator::new();

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
        #[cfg(not(feature = "profile"))]
        HEAP_ALLOCATOR.lock().init(heap_start, heap_size);

        #[cfg(feature = "profile")]
        HEAP_ALLOCATOR.inner.lock().init(heap_start, heap_size);
    }
}

// --- TrackingAllocator (only when profile feature is enabled) ---

#[cfg(feature = "profile")]
pub struct TrackingAllocator {
    inner: LockedHeap,
}

#[cfg(feature = "profile")]
impl TrackingAllocator {
    const fn new() -> Self {
        Self {
            inner: LockedHeap::empty(),
        }
    }

    #[inline(never)]
    fn trace_event(&self, event_type: i32, ptr: i32, size: i32, free: i32) {
        use crate::syscall::{SYSCALL_ALLOC_TRACE, SYSCALL_ARGS, syscall};
        let mut args = [0i32; SYSCALL_ARGS];
        args[0] = event_type;
        args[1] = ptr;
        args[2] = size;
        args[3] = free;
        syscall(SYSCALL_ALLOC_TRACE, &args);
    }

    #[inline(never)]
    fn trace_realloc_event(
        &self,
        old_ptr: i32,
        new_ptr: i32,
        old_size: i32,
        new_size: i32,
        free: i32,
    ) {
        use crate::syscall::{SYSCALL_ALLOC_TRACE, SYSCALL_ARGS, syscall};
        let mut args = [0i32; SYSCALL_ARGS];
        args[0] = crate::syscall::ALLOC_TRACE_REALLOC;
        args[1] = old_ptr;
        args[2] = new_ptr;
        args[3] = old_size;
        args[4] = new_size;
        args[5] = free;
        syscall(SYSCALL_ALLOC_TRACE, &args);
    }
}

#[cfg(feature = "profile")]
unsafe impl core::alloc::GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        let ptr = {
            let mut heap = self.inner.lock();
            heap.allocate_first_fit(layout)
                .ok()
                .map_or(core::ptr::null_mut(), |nn| nn.as_ptr())
        };
        if ptr.is_null() {
            self.trace_event(crate::syscall::ALLOC_TRACE_OOM, 0, layout.size() as i32, 0);
        } else {
            self.trace_event(
                crate::syscall::ALLOC_TRACE_ALLOC,
                ptr as i32,
                layout.size() as i32,
                0,
            );
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: core::alloc::Layout) {
        {
            let mut heap = self.inner.lock();
            unsafe {
                heap.deallocate(core::ptr::NonNull::new_unchecked(ptr), layout);
            }
        }
        self.trace_event(
            crate::syscall::ALLOC_TRACE_DEALLOC,
            ptr as i32,
            layout.size() as i32,
            0,
        );
    }

    unsafe fn realloc(
        &self,
        ptr: *mut u8,
        layout: core::alloc::Layout,
        new_size: usize,
    ) -> *mut u8 {
        let new_layout =
            unsafe { core::alloc::Layout::from_size_align_unchecked(new_size, layout.align()) };
        let new_ptr = {
            let mut heap = self.inner.lock();
            let new_ptr = heap
                .allocate_first_fit(new_layout)
                .ok()
                .map_or(core::ptr::null_mut(), |nn| nn.as_ptr());
            if !new_ptr.is_null() {
                let copy_size = layout.size().min(new_size);
                unsafe {
                    core::ptr::copy_nonoverlapping(ptr, new_ptr, copy_size);
                    heap.deallocate(core::ptr::NonNull::new_unchecked(ptr), layout);
                }
            }
            new_ptr
        };
        if new_ptr.is_null() {
            self.trace_event(crate::syscall::ALLOC_TRACE_OOM, 0, new_size as i32, 0);
        } else {
            self.trace_realloc_event(
                ptr as i32,
                new_ptr as i32,
                layout.size() as i32,
                new_size as i32,
                0,
            );
        }
        new_ptr
    }
}

#[cfg(feature = "profile")]
unsafe impl Sync for TrackingAllocator {}
