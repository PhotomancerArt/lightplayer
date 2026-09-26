//! Retained heap across repeated playlist entry switches.
//!
//! A cycle switches entries for as long as the piece runs, so a switch must
//! give back everything it took: the unloaded subtree's nodes, bindings,
//! registry rows and artifact locations, and whatever the load re-derived.
//! What this pins: after a warm-up, a hundred more `1 → 2 → 1` cycles
//! through `Engine::apply_residency` (with a tick after each switch, the
//! order every edge uses) retain no more heap than the warm state — and a
//! step with no request allocates nothing at all.
//!
//! The entry shaders are attached but never demanded (the stand-in owner
//! renders nothing), so no JIT code is compiled here: this is the engine
//! and registry bookkeeping of a switch. Compiled code across a cycle is
//! plan P6's measurement on the emulated C6.
//!
//! ```bash
//! cargo test -p lpc-engine --test entry_residency_memory -- --nocapture
//! ```
//!
//! The counters are **per thread**: they see only the test's own
//! allocations. Process-wide counters also saw the libtest harness's main
//! thread, which allocates its running-test map and timeout queue right
//! *after* spawning the test thread — bytes a loaded runner can land between
//! two readings (`docs/debt/process-wide-heap-counters-in-tests.md`).

mod entry_residency_support;

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use entry_residency_support::{IDLE, Owner, PATTERN, load, loaded_entries, project_fs};
use lpc_engine::node::ResidencyRequest;

const CYCLES: usize = 100;

#[test]
fn a_hundred_switch_cycles_retain_no_heap() {
    let fs = project_fs();
    let mut rt = load(&fs);
    let owner = Owner::install(rt.engine_mut());

    let cycle = |rt: &mut lpc_engine::engine::LoadedProjectRuntime| {
        for request in [
            ResidencyRequest::switch(IDLE, PATTERN),
            ResidencyRequest::switch(PATTERN, IDLE),
        ] {
            owner.request(request);
            rt.tick_with_residency(&fs, 16).expect("tick");
            owner.take_log();
        }
    };

    // Warm-up: the first cycles size the tree's, registry's and resolver's
    // maps.
    for _ in 0..3 {
        cycle(&mut rt);
    }
    let warm = live();

    for _ in 0..CYCLES {
        cycle(&mut rt);
    }
    let after = live();
    assert_eq!(loaded_entries(rt.engine()), vec![IDLE]);

    // A step with nothing to do: the steady state of every tick.
    let allocs_before = allocs();
    let applied = rt.apply_residency(&fs).expect("no-op step");
    let allocs = allocs() - allocs_before;
    assert!(applied.is_empty());

    println!(
        "live heap after warm-up: {warm} B; after {CYCLES} more 1→2→1 cycles: {after} B \
         (growth {} B); allocations in a no-request step: {allocs}",
        after - warm,
    );
    assert!(
        after <= warm,
        "{CYCLES} switch cycles grew retained heap by {} B",
        after - warm
    );
    assert_eq!(allocs, 0, "a step with no request allocates nothing");
}

// ---- host-heap tracking -------------------------------------------------

/// Counts the heap of the thread that allocates, and only that thread.
struct TrackingAlloc;

thread_local! {
    /// Net bytes this thread allocated minus the bytes it freed. Signed: a
    /// thread may free what another allocated. `const`-initialised with no
    /// destructor, so reaching it from inside the allocator never allocates.
    static LIVE: Cell<isize> = const { Cell::new(0) };
    /// Allocation requests on this thread.
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
}

fn add_live(delta: isize) {
    // `try_with`: a thread tearing down may still allocate or free.
    let _ = LIVE.try_with(|live| live.set(live.get() + delta));
}

unsafe impl GlobalAlloc for TrackingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            add_live(layout.size() as isize);
            let _ = ALLOCS.try_with(|allocs| allocs.set(allocs.get() + 1));
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        add_live(-(layout.size() as isize));
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOC: TrackingAlloc = TrackingAlloc;

/// This thread's live heap.
fn live() -> isize {
    LIVE.with(Cell::get)
}

/// This thread's allocation requests so far.
fn allocs() -> usize {
    ALLOCS.with(Cell::get)
}
