//! Empty runtime state slot root used by nodes without public state.

use lpc_model::{Revision, SlotAccess, SlotDataAccess, SlotShapeId};

/// No runtime state slot root.
#[derive(Clone, Copy, Debug, Default)]
pub struct EmptyRuntimeStateSlots;

impl SlotAccess for EmptyRuntimeStateSlots {
    fn shape_id(&self) -> SlotShapeId {
        SlotShapeId::from_static_name("engine.runtime_state.empty")
    }

    fn data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Unit(Revision::default())
    }
}

pub const EMPTY_RUNTIME_STATE_SLOTS: EmptyRuntimeStateSlots = EmptyRuntimeStateSlots;
