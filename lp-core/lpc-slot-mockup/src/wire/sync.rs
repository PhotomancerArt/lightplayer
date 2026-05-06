use lpc_wire::{WireSlotFullSync, build_slot_full_sync};

use crate::engine::MockRuntime;

pub use lpc_wire::collect_slot_diff as collect_diff;

pub fn full_sync(runtime: &MockRuntime) -> WireSlotFullSync {
    build_slot_full_sync(&runtime.registry, runtime.roots())
}
