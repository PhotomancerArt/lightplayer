//! `UiAction` builders for slot-edit dispatch from field components.
//!
//! Fields are stateless views (ADR D5): input dispatches a `SlotEditOp`
//! against the project controller through the shared `on_action` conduit; the
//! studio actor coalesces queued `SetValue`s per address, and the edit buffer
//! plus overlay mirror keep the DTO value stable until the server acks.

use lpa_studio_core::{
    ControllerId, LpValue, ProjectController, ProjectSlotAddress, SlotEditOp, UiAction,
};

/// Build the `SetValue` action a field dispatches on input.
pub(crate) fn slot_set_value_action(address: ProjectSlotAddress, value: LpValue) -> UiAction {
    UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        SlotEditOp::SetValue { address, value },
    )
}

/// Build the per-slot revert action (labelled "Reset" on live rows).
pub(crate) fn slot_revert_action(address: ProjectSlotAddress) -> UiAction {
    UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        SlotEditOp::Revert { address },
    )
}
