//! Generic slot sync and mutation wire payloads.

mod access_sync;
mod mutation;
mod slot_shape_registry_json;
mod sync;

pub use access_sync::{
    build_slot_full_sync, build_slot_roots_snapshot, collect_slot_diff, snapshot_slot_root,
    snapshot_slot_shape,
};
pub use lpc_model::slot_codec::{
    ArrayReader, JsonSyntaxSource, ObjectReader, PropReader, SlotReader, SourceSpan, SyntaxError,
    SyntaxEvent, SyntaxEventSource, TomlSyntaxSource, ValueReader,
};
pub use mutation::{
    WireSlotMutationId, WireSlotMutationOp, WireSlotMutationRejection, WireSlotMutationRequest,
    WireSlotMutationResponse, WireSlotMutationResult,
};
pub use slot_shape_registry_json::write_slot_shape_registry_snapshot_json;
pub use sync::{
    WireSlotChange, WireSlotFullSync, WireSlotPatch, WireSlotRootSnapshot, WireSlotRootsSnapshot,
};
