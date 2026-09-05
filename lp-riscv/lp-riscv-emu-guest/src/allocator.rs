//! Global allocator setup for emulator guest code
//!
//! When the `profile` feature is enabled, wraps the allocator with a
//! `TrackingAllocator` that emits a syscall on every alloc/dealloc/realloc.
//! The host emulator captures these events for offline analysis.

#[cfg(feature = "profile")]
extern crate alloc;

use linked_list_allocator::LockedHeap;

#[cfg(feature = "profile")]
use core::sync::atomic::{AtomicBool, Ordering};

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

    #[cfg(feature = "profile")]
    lp_perf::set_marker_shape_hook(emit_free_list_shape);
}

// --- TrackingAllocator (only when profile feature is enabled) ---

#[cfg(feature = "profile")]
pub struct TrackingAllocator {
    inner: LockedHeap,
}

/// Set for the duration of [`walk_and_emit_free_list_shape`]'s own
/// alloc/dealloc traffic so it does not trace itself: the walk allocates
/// and frees tens of thousands of units, and none of that is a real guest
/// allocation the trace should record.
#[cfg(feature = "profile")]
static WALK_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

#[cfg(feature = "profile")]
impl TrackingAllocator {
    const fn new() -> Self {
        Self {
            inner: LockedHeap::empty(),
        }
    }

    #[inline(never)]
    fn trace_event(&self, event_type: i32, ptr: i32, size: i32, free: i32) {
        self.trace_event_aligned(event_type, ptr, size, free, 0);
    }

    /// [`Self::trace_event`] plus the request's `Layout::align`, which only
    /// the alloc path has and only the host's fragmentation replay wants.
    /// An `align` of 0 means "not applicable" (dealloc, OOM).
    #[inline(never)]
    fn trace_event_aligned(&self, event_type: i32, ptr: i32, size: i32, free: i32, align: i32) {
        if WALK_IN_PROGRESS.load(Ordering::Relaxed) {
            return;
        }
        use crate::syscall::{SYSCALL_ALLOC_TRACE, SYSCALL_ARGS, syscall};
        let mut args = [0i32; SYSCALL_ARGS];
        args[0] = event_type;
        args[1] = ptr;
        args[2] = size;
        args[3] = free;
        args[4] = align;
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
        align: i32,
    ) {
        if WALK_IN_PROGRESS.load(Ordering::Relaxed) {
            return;
        }
        use crate::syscall::{SYSCALL_ALLOC_TRACE, SYSCALL_ARGS, syscall};
        let mut args = [0i32; SYSCALL_ARGS];
        args[0] = crate::syscall::ALLOC_TRACE_REALLOC;
        args[1] = old_ptr;
        args[2] = new_ptr;
        args[3] = old_size;
        args[4] = new_size;
        args[5] = free;
        args[6] = align;
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
            self.trace_event_aligned(
                crate::syscall::ALLOC_TRACE_ALLOC,
                ptr as i32,
                layout.size() as i32,
                0,
                layout.align() as i32,
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
                new_layout.align() as i32,
            );
        }
        new_ptr
    }
}

#[cfg(feature = "profile")]
unsafe impl Sync for TrackingAllocator {}

/// Installed via `lp_perf::set_marker_shape_hook` in [`init_heap`]. Runs on
/// the guest's own stack, synchronously, right after a marker `ecall`
/// returns `1` — the caller of `lp_perf::emit_begin!`/`emit_end!` does not
/// regain control until this returns.
#[cfg(feature = "profile")]
fn emit_free_list_shape() {
    walk_and_emit_free_list_shape();
}

/// Read the guest's free list exactly and emit it as a run of
/// `SYSCALL_ALLOC_TRACE` calls, ending with `ALLOC_TRACE_FREE_LIST_END`.
///
/// Ported from `fw-esp32v3::recovery::panic_path::free_list_shape`: take the
/// smallest block the allocator will hand out over and over until it
/// refuses, then give every block back. First-fit over an address-sorted
/// free list returns addresses ascending, so a run of blocks with no gap is
/// exactly one hole and a gap is exactly one still-allocated block. Unlike
/// the panic-path probe this does not cap the number of holes at a
/// `MAX_RUNS` — a fragmentation measurement that silently stops counting
/// past N holes would corrupt the very ratchet it feeds, and the trace has
/// nowhere to record "truncated" that the ratchet would honor. Runs are
/// therefore emitted as they are found, and every allocated unit is linked
/// into an intrusive list using its own (otherwise uninitialized) storage —
/// no side array, so there is no cap to hit.
///
/// ⚠️ Briefly owns every free byte in the heap; nothing else may allocate
/// while it runs. The guest is single-threaded and this only runs from the
/// perf-marker syscall's own call stack, so that holds today.
#[cfg(feature = "profile")]
fn walk_and_emit_free_list_shape() {
    use crate::syscall::{
        ALLOC_TRACE_FREE_LIST_END, ALLOC_TRACE_FREE_RUN, SYSCALL_ALLOC_TRACE, SYSCALL_ARGS, syscall,
    };

    /// The block size `linked_list_allocator` actually hands out for a
    /// request this small on this 32-bit target: `size_of::<Hole>()`, i.e.
    /// `2 * size_of::<usize>()` = 8 B. Requesting exactly `size_of::<usize>()`
    /// (rather than 1, as the panic-path probe does) means the returned
    /// block is provably large enough to hold the `usize` this walk writes
    /// into it below, with no reliance on the allocator's rounding-up
    /// behavior for the write to be in-bounds — only for `STEP` to match it.
    const STEP: usize = 2 * core::mem::size_of::<usize>();

    let Ok(unit) = core::alloc::Layout::from_size_align(
        core::mem::size_of::<usize>(),
        core::mem::align_of::<usize>(),
    ) else {
        return;
    };

    WALK_IN_PROGRESS.store(true, Ordering::Relaxed);

    let mut holes: u32 = 0;
    let mut largest: u32 = 0;
    let mut total_free: u32 = 0;
    let mut run_start: usize = 0;
    let mut run_len: usize = 0;
    let mut last_end: usize = 0;
    // Head of an intrusive LIFO list of every unit taken during the walk,
    // threaded through the units' own storage (`*ptr = previous head`), so
    // freeing them all afterward costs no extra heap or stack space.
    let mut list_head: usize = 0;

    let emit_run = |start: u32, len: u32| {
        let mut args = [0i32; SYSCALL_ARGS];
        args[0] = ALLOC_TRACE_FREE_RUN;
        args[1] = start as i32;
        args[2] = len as i32;
        syscall(SYSCALL_ALLOC_TRACE, &args);
    };

    loop {
        // SAFETY: `unit` has non-zero size; every pointer returned here is
        // either linked into `list_head` (read back and freed with this
        // same layout below) or, on a null return, not touched at all.
        let raw = unsafe { alloc::alloc::alloc(unit) };
        let ptr = raw as usize;
        if ptr == 0 {
            break;
        }

        // SAFETY: `unit`'s size is `size_of::<usize>()` and its alignment
        // divides `align_of::<usize>()`, so `raw` is valid and aligned for
        // one `usize` write, and this is the first write to memory the
        // allocator just handed back.
        unsafe { (raw as *mut usize).write(list_head) };
        list_head = ptr;

        if run_len > 0 && ptr == last_end {
            run_len += STEP;
        } else {
            if run_len > 0 {
                emit_run(run_start as u32, run_len as u32);
                holes += 1;
                total_free += run_len as u32;
                largest = largest.max(run_len as u32);
            }
            run_start = ptr;
            run_len = STEP;
        }
        last_end = ptr + STEP;
    }

    if run_len > 0 {
        emit_run(run_start as u32, run_len as u32);
        holes += 1;
        total_free += run_len as u32;
        largest = largest.max(run_len as u32);
    }

    {
        let mut args = [0i32; SYSCALL_ARGS];
        args[0] = ALLOC_TRACE_FREE_LIST_END;
        args[1] = holes as i32;
        args[2] = largest as i32;
        args[3] = total_free as i32;
        syscall(SYSCALL_ALLOC_TRACE, &args);
    }

    // Give every unit back, walking the intrusive list built above.
    let mut p = list_head;
    while p != 0 {
        // SAFETY: `p` was returned by `alloc::alloc::alloc(unit)` above and
        // has not been freed yet; the `usize` at its start is the previous
        // list node (or 0), written before `p` was linked in.
        let prev = unsafe { (p as *const usize).read() };
        unsafe { alloc::alloc::dealloc(p as *mut u8, unit) };
        p = prev;
    }

    WALK_IN_PROGRESS.store(false, Ordering::Relaxed);
}
