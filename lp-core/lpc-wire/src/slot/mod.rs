//! Generic slot sync and mutation wire payloads.

mod access_sync;
mod mutation;
mod sync;

pub use access_sync::{
    build_slot_full_sync, collect_slot_diff, snapshot_slot_root, snapshot_slot_shape,
};
pub use mutation::{
    WireSlotMutationId, WireSlotMutationOp, WireSlotMutationRejection, WireSlotMutationRequest,
    WireSlotMutationResponse, WireSlotMutationResult,
};
pub use sync::{WireSlotChange, WireSlotFullSync, WireSlotPatch, WireSlotRootSnapshot};
