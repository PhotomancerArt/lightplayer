//! Engine-side produced slot access and runtime state surfaces.

mod produced_slot_access;

pub use produced_slot_access::{
    EMPTY_PRODUCED_SLOTS, EMPTY_RUNTIME_STATE, EmptyProducedSlots, EmptyRuntimeState,
    ProducedSlotAccess, ProducedSlotEntry, RuntimeStateAccess,
};
