//! `UiAction` builders for slot-edit dispatch from field components.
//!
//! Fields are stateless views (ADR D5): input dispatches a `SlotEditOp`
//! against the project controller through the shared `on_action` conduit; the
//! studio actor coalesces queued `SetValue`s per address, and the edit buffer
//! plus overlay mirror keep the DTO value stable until the server acks.

use lpa_studio_core::{
    ControllerId, LpValue, ProjectController, ProjectSlotAddress, SlotEditOp, SlotMapKey, UiAction,
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

/// Build the structural add gesture (map entry add, option on, enum variant
/// switch): the server constructs the defaults at `address` (M3 D1).
pub(crate) fn slot_ensure_present_action(address: ProjectSlotAddress) -> UiAction {
    UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        SlotEditOp::EnsurePresent { address },
    )
}

/// Build the structural remove gesture (map entry remove, option off).
pub(crate) fn slot_remove_value_action(address: ProjectSlotAddress) -> UiAction {
    UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        SlotEditOp::RemoveValue { address },
    )
}

/// Build the map entry key move gesture: `address` is the **map** slot, and
/// the server re-keys the `from_key` entry to `to_key` (rejecting an
/// occupied target).
pub(crate) fn slot_move_entry_action(
    address: ProjectSlotAddress,
    from_key: SlotMapKey,
    to_key: SlotMapKey,
) -> UiAction {
    UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        SlotEditOp::MoveEntry {
            address,
            from_key,
            to_key,
        },
    )
}
