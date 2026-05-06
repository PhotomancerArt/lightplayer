//! Generic slot sync and mutation wire payloads.

mod mutation;
mod sync;

pub use mutation::{
    WireSlotMutationId, WireSlotMutationOp, WireSlotMutationRejection, WireSlotMutationRequest,
    WireSlotMutationResponse, WireSlotMutationResult,
};
pub use sync::{WireSlotChange, WireSlotFullSync, WireSlotPatch, WireSlotRootSnapshot};
