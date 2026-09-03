//! Test-only global allocator that counts requests per thread.
//!
//! "How many allocations does a steady frame make?" is a question the
//! emulator's heap-budget gate answers in CI (`docs/heap-budget-gate.md`),
//! but an emulator run is minutes; this makes it a unit test. The crate's
//! test binary installs [`CountingAlloc`] as its `#[global_allocator]` (see
//! `lib.rs`), and [`measure`] brackets a closure with a snapshot.
//!
//! Counters are **thread-local** because `cargo test` runs tests on parallel
//! threads: a process-wide counter would attribute one test's churn to
//! another. The thread-local cells are `const`-initialised so touching them
//! from inside the allocator never allocates.

use core::alloc::{GlobalAlloc, Layout};
use core::cell::Cell;
use std::alloc::System;
use std::thread_local;

/// The counting wrapper around the system allocator.
pub(crate) struct CountingAlloc;

thread_local! {
    static ALLOCS: Cell<u64> = const { Cell::new(0) };
    static BYTES: Cell<u64> = const { Cell::new(0) };
    static DEALLOCS: Cell<u64> = const { Cell::new(0) };
}

/// Allocation activity on the current thread.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct AllocSnapshot {
    /// Allocation requests (`alloc`, `alloc_zeroed`, `realloc`).
    pub allocs: u64,
    /// Bytes requested by those calls.
    pub bytes: u64,
    /// `dealloc` calls.
    pub deallocs: u64,
}

impl AllocSnapshot {
    fn since(self, earlier: Self) -> Self {
        Self {
            allocs: self.allocs - earlier.allocs,
            bytes: self.bytes - earlier.bytes,
            deallocs: self.deallocs - earlier.deallocs,
        }
    }
}

/// The current thread's counters so far.
pub(crate) fn snapshot() -> AllocSnapshot {
    AllocSnapshot {
        allocs: ALLOCS.try_with(Cell::get).unwrap_or(0),
        bytes: BYTES.try_with(Cell::get).unwrap_or(0),
        deallocs: DEALLOCS.try_with(Cell::get).unwrap_or(0),
    }
}

/// Run `f` and report the allocation activity it caused on this thread.
pub(crate) fn measure<R>(f: impl FnOnce() -> R) -> (R, AllocSnapshot) {
    let before = snapshot();
    let result = f();
    (result, snapshot().since(before))
}

fn record_request(size: usize) {
    // `try_with`: a thread tearing down may still free; a counter that cannot
    // be reached is simply not bumped.
    let _ = ALLOCS.try_with(|c| c.set(c.get() + 1));
    let _ = BYTES.try_with(|c| c.set(c.get() + size as u64));
}

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record_request(layout.size());
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record_request(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let _ = DEALLOCS.try_with(|c| c.set(c.get() + 1));
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record_request(new_size);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    #[test]
    fn measure_counts_only_the_closure_on_this_thread() {
        let (_, quiet) = measure(|| 1 + 1);
        assert_eq!(quiet.allocs, 0);

        let (v, busy) = measure(|| {
            let mut v: Vec<u32> = Vec::with_capacity(4);
            v.push(1);
            v
        });
        assert_eq!(busy.allocs, 1, "one Vec allocation");
        assert_eq!(busy.bytes, 16);
        drop(v);
    }
}
