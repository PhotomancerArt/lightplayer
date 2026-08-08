//! `UiAction` builders for slot-edit dispatch from field components.
//!
//! Fields are stateless views (ADR D5): input dispatches a `SlotEditOp`
//! against the project controller through the shared `on_action` conduit; the
//! studio actor coalesces queued `SetValue`s per address, and the edit buffer
//! plus overlay mirror keep the DTO value stable until the server acks.

use lpa_studio_core::{
    ControllerId, LpValue, NodeClearDebugOp, PanelAutoSaveOp, PanelClearOp, PanelWriteOp,
    ProjectController, ProjectNodeAddress, ProjectSlotAddress, SlotEditOp, SlotMapKey, UiAction,
    UiPanelTarget,
};

/// Build the `SetValue` action a field dispatches on input.
pub(crate) fn slot_set_value_action(address: ProjectSlotAddress, value: LpValue) -> UiAction {
    UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        SlotEditOp::SetValue { address, value },
    )
}

/// Build the gesture action for a panel widget: a control with a panel
/// target writes its `(scope, channel)` down the runtime command channel
/// (panel.md P8 — nothing staged, nothing dirty, the actor coalesces per
/// target); one without edits the authored default through the slot path.
pub(crate) fn panel_or_slot_action(
    panel_target: &Option<UiPanelTarget>,
    address: ProjectSlotAddress,
    value: LpValue,
) -> UiAction {
    panel_write_or_slot_action(panel_target.as_ref(), Some(&address), value)
        .expect("an address is always a dispatch")
}

/// [`panel_or_slot_action`] for one DIMENSION of a grouped control, whose
/// slot address may be absent (a wired row the projection marked read-only).
///
/// The target-or-address fallback is per dimension, not per control (the
/// clock's Transport, P8): a wired dimension writes its channel, an unwired
/// one edits its own slot, and one with neither has nothing to dispatch —
/// `None`, so the widget renders inert rather than firing a dead handler.
pub(crate) fn panel_write_or_slot_action(
    panel_target: Option<&UiPanelTarget>,
    address: Option<&ProjectSlotAddress>,
    value: LpValue,
) -> Option<UiAction> {
    match panel_target {
        Some(target) => Some(UiAction::from_op(
            ControllerId::new(ProjectController::NODE_ID),
            PanelWriteOp {
                scope: target.scope,
                channel: target.channel.clone(),
                value,
                ttl_ms: None,
            },
        )),
        None => Some(slot_set_value_action(address?.clone(), value)),
    }
}

/// Build the per-control clear (panel.md P2): release the engaged panel
/// writer and return the control to Read.
pub(crate) fn panel_clear_action(target: &UiPanelTarget) -> UiAction {
    UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        PanelClearOp {
            request: lpc_wire::WirePanelClearRequest::Channel {
                scope: target.scope,
                channel: target.channel.clone(),
            },
        },
    )
}

/// Build the per-scope panel reset (panel.md P2 clear at scope
/// granularity; descends per settled P-Q4) — the module panel's ↺.
pub(crate) fn panel_clear_scope_action(scope: lpc_wire::WireScopeRef) -> UiAction {
    UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        PanelClearOp {
            request: lpc_wire::WirePanelClearRequest::Scope { scope },
        },
    )
}

/// Build the panel-state auto-save toggle (panel.md P11). Project-level,
/// so it takes no scope — the root module's panel is the one that presents
/// it, and the server's answer arrives on the next read rather than being
/// applied locally.
pub(crate) fn panel_auto_save_action(enabled: bool) -> UiAction {
    UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        PanelAutoSaveOp { enabled },
    )
}

/// Build the per-slot revert action for a persisted (unsaved) edit.
pub(crate) fn slot_revert_action(address: ProjectSlotAddress) -> UiAction {
    UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        SlotEditOp::Revert { address },
    )
}

/// Build the per-value **Clear** action for a debug (live-only) override
/// (D7): the same `RemoveSlotEdit` mechanism as revert, under the verb debug
/// values use — for a Debug slot the authored default IS the shape default.
pub(crate) fn slot_clear_action(address: ProjectSlotAddress) -> UiAction {
    UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        SlotEditOp::Clear { address },
    )
}

/// Build the **per-node** Clear action ([`NodeClearDebugOp`], D7's node
/// scope): every Debug override under one node's subtree goes, persisted
/// edits stay. The Debug section header is its only entry point — a node's
/// debug territory owns its own reset.
///
/// `None` when the card's path is not a parsable node address (story
/// fixtures with stand-in paths), so the header simply renders no Clear.
pub(crate) fn node_clear_debug_action(node: &str) -> Option<UiAction> {
    let node = ProjectNodeAddress::parse(node).ok()?;
    Some(UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        NodeClearDebugOp { node },
    ))
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

#[cfg(test)]
mod tests {
    use lpa_studio_core::{
        LpValue, NodeId, PanelWriteOp, ProjectNodeAddress, ProjectSlotAddress, ProjectSlotRoot,
        SlotEditOp, SlotPath, UiPanelTarget,
    };

    use super::panel_write_or_slot_action;

    /// The per-dimension target-or-address fallback (P8): a wired dimension
    /// writes its channel, an unwired one edits its own slot, and one with
    /// neither has nothing to dispatch — the three states the clock's
    /// transport can be in leaf by leaf.
    #[test]
    fn a_dimension_writes_its_channel_else_edits_its_slot_else_nothing() {
        let target = UiPanelTarget {
            scope: lpc_wire::WireScopeRef::Module {
                owner: NodeId::new(1),
            },
            channel: "clock.rate".to_string(),
            engaged: false,
        };
        let address = ProjectSlotAddress::new(
            ProjectNodeAddress::parse("/demo.module/node.clock").expect("valid address"),
            ProjectSlotRoot::def(),
            SlotPath::parse("transport.rate").expect("valid path"),
        );

        // Wired: the channel wins even though the slot address rides along
        // as the fallback.
        let wired = panel_write_or_slot_action(Some(&target), Some(&address), LpValue::F32(2.0))
            .expect("a wired dimension dispatches");
        assert_eq!(
            wired.op_as::<PanelWriteOp>().map(|op| op.channel.clone()),
            Some("clock.rate".to_string())
        );

        // Unwired: the slot edit at this dimension's OWN address.
        let unwired = panel_write_or_slot_action(None, Some(&address), LpValue::F32(2.0))
            .expect("an unwired dimension still edits its slot");
        assert!(matches!(
            unwired.op_as::<SlotEditOp>(),
            Some(SlotEditOp::SetValue { address: at, .. }) if *at == address
        ));

        // Neither: a read-only, unwired dimension is inert.
        assert!(panel_write_or_slot_action(None, None, LpValue::F32(2.0)).is_none());
    }
}
