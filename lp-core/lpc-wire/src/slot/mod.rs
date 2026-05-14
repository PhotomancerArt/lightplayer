//! Generic slot sync and mutation wire payloads.

mod access_sync;
mod authored_toml;
mod mutation;
pub mod native;
mod slot_data_json;
mod slot_shape_registry_json;
mod sync;

pub use access_sync::{
    build_slot_full_sync, build_slot_roots_snapshot, collect_slot_diff, snapshot_slot_root,
    snapshot_slot_shape,
};
pub use authored_toml::{
    SlotTomlError, decode_slot_data_toml, decode_slot_data_toml_with_ignored_fields,
    encode_slot_data_access_toml, encode_slot_data_toml,
};
pub use mutation::{
    WireSlotMutationId, WireSlotMutationOp, WireSlotMutationRejection, WireSlotMutationRequest,
    WireSlotMutationResponse, WireSlotMutationResult,
};
pub use native::{
    JsonSyntaxSource, SlotJsonArray, SlotJsonObject, SlotJsonValue, SlotJsonWriter, SlotReader,
    SourceSpan, SyntaxError, SyntaxEvent, SyntaxEventSource, TomlSyntaxSource,
};
pub use slot_data_json::write_slot_data_json;
pub use slot_shape_registry_json::write_slot_shape_registry_snapshot_json;
pub use sync::{
    WireSlotChange, WireSlotFullSync, WireSlotPatch, WireSlotRootSnapshot, WireSlotRootsSnapshot,
};
