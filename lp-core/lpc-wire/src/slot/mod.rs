//! Generic slot sync wire payloads.

mod access_sync;
mod slot_shape_registry_json;
mod sync;

pub use access_sync::{
    build_slot_full_sync, build_slot_roots_snapshot, collect_slot_diff, snapshot_slot_root,
    snapshot_slot_shape, wire_slot_data_from_slot_access,
};
pub use slot_shape_registry_json::write_slot_shape_registry_snapshot_json;
pub use sync::{
    WireSlotChange, WireSlotData, WireSlotFullSync, WireSlotPatch, WireSlotRootSnapshot,
    WireSlotRootsSnapshot,
};
