//! Slot-level edit operations dispatched from Studio field components.

use core::any::Any;

use lpc_model::LpValue;

use crate::{
    ActionClass, ActionMeta, ActionPriority, ControllerOp, PROJECT_EDITOR_ACTION_DEADLINE,
    ProjectSlotAddress,
};

/// A slot edit targeting one addressed project slot.
///
/// Field components dispatch these as `UiAction`s against
/// `ProjectController::NODE_ID`; the op carries the full slot address, so no
/// per-slot controller id is needed. The studio actor coalesces queued
/// `SetValue`s per address (latest wins) to absorb `oninput` floods; every
/// other variant — `Revert` and the structural gestures — never coalesces
/// and acts as a coalescing barrier.
///
/// The structural gestures mirror the wire vocabulary (M3 decision D1): the
/// client never composes composite values, it sends `EnsurePresent`/`Remove`
/// and lets the server construct defaults.
///
/// Note this is the Studio *controller op*; the wire-level pending-edit
/// operation of the same name lives in `lpc_model::SlotEditOp`.
#[derive(Clone, Debug, PartialEq)]
pub enum SlotEditOp {
    /// Stage `value` as the pending edit for the slot at `address`.
    SetValue {
        address: ProjectSlotAddress,
        value: LpValue,
    },
    /// Structural gesture: ensure the slot at `address` exists in the
    /// effective def (map entry add, option on, enum variant switch), with
    /// server-constructed defaults.
    EnsurePresent { address: ProjectSlotAddress },
    /// Structural gesture: remove the slot at `address` from the effective
    /// def (map entry remove, option off). Distinct from [`Self::Revert`],
    /// which removes the *overlay entry* at the address instead.
    RemoveValue { address: ProjectSlotAddress },
    /// Discard the pending edit for the slot at `address`, locally and on
    /// the server overlay.
    Revert { address: ProjectSlotAddress },
}

impl SlotEditOp {
    /// The slot address this edit targets.
    pub fn address(&self) -> &ProjectSlotAddress {
        match self {
            Self::SetValue { address, .. }
            | Self::EnsurePresent { address }
            | Self::RemoveValue { address }
            | Self::Revert { address } => address,
        }
    }
}

impl ControllerOp for SlotEditOp {
    fn default_action_meta(&self) -> ActionMeta {
        match self {
            Self::SetValue { .. } => ActionMeta::new(
                "Set value",
                "Stage a new value for this slot as a pending edit.",
                ActionPriority::Primary,
            ),
            Self::EnsurePresent { .. } => ActionMeta::new(
                "Add",
                "Create or activate this slot with server defaults as a pending edit.",
                ActionPriority::Primary,
            ),
            Self::RemoveValue { .. } => ActionMeta::new(
                "Remove",
                "Remove this slot from the effective definition as a pending edit.",
                ActionPriority::Primary,
            ),
            Self::Revert { .. } => ActionMeta::new(
                "Revert",
                "Discard the pending edit for this slot.",
                ActionPriority::Secondary,
            ),
        }
    }

    fn action_class(&self) -> ActionClass {
        // Slot edits are ordinary editor foreground ops: they preempt a
        // passive refresh but not each other, on the editor quiet-gap budget.
        ActionClass::Foreground {
            deadline: PROJECT_EDITOR_ACTION_DEADLINE,
        }
    }

    fn clone_box(&self) -> Box<dyn ControllerOp> {
        Box::new(self.clone())
    }

    fn eq_op(&self, other: &dyn ControllerOp) -> bool {
        other.as_any().downcast_ref::<Self>() == Some(self)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

#[cfg(test)]
mod tests {
    use lpc_model::SlotPath;

    use crate::{ProjectNodeAddress, ProjectSlotRoot};

    use super::*;

    fn test_address() -> ProjectSlotAddress {
        ProjectSlotAddress::new(
            ProjectNodeAddress::parse("/demo.project/clock.clock").unwrap(),
            ProjectSlotRoot::def(),
            SlotPath::parse("controls.rate").unwrap(),
        )
    }

    #[test]
    fn slot_edit_ops_are_editor_foreground_class() {
        let ops = [
            SlotEditOp::SetValue {
                address: test_address(),
                value: LpValue::F32(2.0),
            },
            SlotEditOp::EnsurePresent {
                address: test_address(),
            },
            SlotEditOp::RemoveValue {
                address: test_address(),
            },
            SlotEditOp::Revert {
                address: test_address(),
            },
        ];

        for op in ops {
            assert_eq!(
                op.action_class(),
                ActionClass::Foreground {
                    deadline: PROJECT_EDITOR_ACTION_DEADLINE,
                },
                "{op:?}"
            );
            assert_eq!(op.address(), &test_address());
        }
    }
}
