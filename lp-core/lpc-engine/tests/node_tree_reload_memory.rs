//! The node tree's retained heap across repeated attach and remove of one
//! subtree.
//!
//! Dormant playlist entries remove the playing entry's subtree and attach
//! the next one on every pattern switch. Until 2026-09-25 the tree stored
//! its entries as a dense `Vec<Option<_>>` indexed by id: a removed node
//! left a `None` tombstone, ids are never reused, so every reload grew the
//! vector by the subtree's size, forever
//! (`docs/defects/2026-09-25-node-tree-tombstones-grow-per-reload.md`).
//!
//! What this pins: after a warm-up reload, a hundred more reloads of the
//! same three-node subtree retain **zero** additional heap, while ids keep
//! climbing (never reused).
//!
//! ```bash
//! cargo test -p lpc-engine --test node_tree_reload_memory -- --nocapture
//! ```
//!
//! The counter is **per thread**: it sees only the test's own allocations.
//! A process-wide counter also saw the libtest harness's main thread, which
//! allocates its running-test map and timeout queue right *after* spawning
//! the test thread — about 900 B that a loaded runner can land between the
//! two readings (`docs/debt/process-wide-heap-counters-in-tests.md`).

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use lpc_engine::node::RuntimeNodeTree;
use lpc_model::{ArtifactSpec, NodeId, NodeInvocation, NodeName, Revision, TreePath};
use lpc_wire::{WireChildKind, WireSlotIndex};

const RELOADS: u32 = 100;

#[test]
fn reloading_a_subtree_retains_no_heap() {
    let mut tree: RuntimeNodeTree<()> =
        RuntimeNodeTree::new(TreePath::parse("/root.show").unwrap(), Revision::new(0));
    let mut frame = 1;

    // Warm-up: the first reloads size the tree's maps and indices.
    for _ in 0..2 {
        reload(&mut tree, &mut frame);
    }
    let warm = live();
    let warm_next_id = tree.next_id();

    for _ in 0..RELOADS {
        reload(&mut tree, &mut frame);
    }
    let after = live();

    println!(
        "live heap after warm-up: {warm} B; after {RELOADS} more reloads: {after} B \
         (growth {} B); next id {warm_next_id} -> {}",
        after - warm,
        tree.next_id()
    );

    assert_eq!(tree.len(), 1, "only the root is left after each reload");
    assert_eq!(
        tree.next_id(),
        warm_next_id + RELOADS * SUBTREE_NODES,
        "every reload allocates fresh ids; none is reused"
    );
    assert!(
        after <= warm,
        "{RELOADS} reloads grew retained heap by {} B",
        after - warm
    );
}

const SUBTREE_NODES: u32 = 3;

/// Attach a three-node subtree (a pattern and two children) under the root,
/// then remove it, as a playlist switch unloads one entry.
fn reload(tree: &mut RuntimeNodeTree<()>, frame: &mut i64) {
    let root = tree.root();
    let pattern = add(tree, root, "pattern", "module", *frame);
    add(tree, pattern, "shader", "shader", *frame);
    add(tree, pattern, "clock", "clock", *frame);
    *frame += 1;
    tree.remove_subtree(pattern, Revision::new(*frame)).unwrap();
    *frame += 1;
}

fn add(tree: &mut RuntimeNodeTree<()>, parent: NodeId, name: &str, ty: &str, frame: i64) -> NodeId {
    tree.add_child(
        parent,
        NodeName::parse(name).unwrap(),
        NodeName::parse(ty).unwrap(),
        WireChildKind::Input {
            source: WireSlotIndex(0),
        },
        NodeInvocation::new(ArtifactSpec::path("child.lp")),
        Revision::new(frame),
    )
    .unwrap()
}

// ---- host-heap tracking -------------------------------------------------

/// Counts the live heap of the thread that allocates, and only that thread.
struct TrackingAlloc;

thread_local! {
    /// Net bytes this thread allocated minus the bytes it freed. Signed: a
    /// thread may free what another allocated. `const`-initialised with no
    /// destructor, so reaching it from inside the allocator never allocates.
    static LIVE: Cell<isize> = const { Cell::new(0) };
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
