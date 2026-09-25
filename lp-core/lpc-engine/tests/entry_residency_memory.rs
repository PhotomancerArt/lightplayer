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
//! ⚠️ One `#[test]` per binary — the allocator counters are process-wide.

mod entry_residency_support;

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

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
    let allocs_before = ALLOCS.load(Ordering::Relaxed);
    let applied = rt.apply_residency(&fs).expect("no-op step");
    let allocs = ALLOCS.load(Ordering::Relaxed) - allocs_before;
    assert!(applied.is_empty());

    println!(
        "live heap after warm-up: {warm} B; after {CYCLES} more 1→2→1 cycles: {after} B \
         (growth {} B); allocations in a no-request step: {allocs}",
        after as isize - warm as isize,
    );
    assert!(
        after <= warm,
        "{CYCLES} switch cycles grew retained heap by {} B",
        after - warm
    );
    assert_eq!(allocs, 0, "a step with no request allocates nothing");
}

// ---- host-heap tracking -------------------------------------------------

struct TrackingAlloc;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static ALLOCS: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for TrackingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            LIVE.fetch_add(layout.size(), Ordering::Relaxed);
            ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOC: TrackingAlloc = TrackingAlloc;

fn live() -> usize {
    LIVE.load(Ordering::Relaxed)
}
