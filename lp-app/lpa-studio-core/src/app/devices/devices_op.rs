//! The devices page's ONE verb: carry an [`Action`] to the roster.
//!
//! Every device gesture in the UI is an `lpa-devices` [`Action`] and nothing
//! else. There is deliberately no per-verb op enum here: the old system's
//! `DeviceOp` grew a variant per flow and each variant grew its own state,
//! which is the disease the rebuilt model exists to cure. The op is a thin
//! envelope so the existing [`UiAction`](crate::UiAction) dispatch (node id +
//! downcast) can carry a model action, and the model's own vocabulary stays
//! the only device vocabulary.

use core::any::Any;

use lpa_devices::Action;

use crate::{ActionClass, ActionConfirmation, ActionMeta, ActionPriority, ControllerOp};

/// One device gesture, verbatim from the model's action vocabulary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DevicesOp(pub Action);

impl DevicesOp {
    /// The node id device actions target. Routed by `StudioController`
    /// directly — there is no controller struct behind it, only the roster.
    pub const NODE_ID: &'static str = "studio|devices";

    /// The action this op carries.
    pub fn action(&self) -> &Action {
        &self.0
    }

    /// This op as a dispatchable [`UiAction`](crate::UiAction).
    pub fn action_for(action: Action) -> crate::UiAction {
        crate::UiAction::from_op(crate::ControllerId::new(Self::NODE_ID), Self(action))
    }
}

impl ControllerOp for DevicesOp {
    fn default_action_meta(&self) -> ActionMeta {
        match &self.0 {
            Action::AddFromUsb => ActionMeta::new(
                "Add a device",
                "Pick a USB port to look for a LightPlayer board on.",
                ActionPriority::Primary,
            )
            .with_icon("add"),
            Action::AdoptLink { .. } => ActionMeta::new(
                "Set up this device",
                "Remember this board so it can be set up.",
                ActionPriority::Primary,
            ),
            Action::DismissLink { .. } => ActionMeta::new(
                "Dismiss",
                "Stop looking at this port and hand the grant back.",
                ActionPriority::Tertiary,
            )
            .destructive()
            .with_confirmation(
                ActionConfirmation::new(
                    "Dismiss this port?",
                    "Studio hands the browser's permission for this port back. \
                     You can pick it again from Add a device.",
                    "Dismiss",
                )
                .inline(),
            ),
            Action::Connect { .. } => ActionMeta::new(
                "Connect",
                "Open the port and ask the board what it is.",
                ActionPriority::Primary,
            ),
            Action::Reconnect { .. } => ActionMeta::new(
                "Reconnect…",
                "Pick this board's port again. Some boards can't be \
                 re-recognized after a replug, so the browser asks once more.",
                ActionPriority::Primary,
            ),
            Action::Disconnect { .. } => ActionMeta::new(
                "Disconnect",
                "Close the port. The board keeps running; Studio stops watching it.",
                ActionPriority::Secondary,
            ),
            Action::Forget { .. } => ActionMeta::new(
                "Forget",
                "Remove this device, what Studio remembers about it, and the \
                 browser's permission for its port.",
                ActionPriority::Tertiary,
            )
            .destructive()
            .with_confirmation(
                ActionConfirmation::new(
                    "Forget this device?",
                    "Studio removes the device, its remembered name, and the \
                     browser's permission for its port. Nothing on the board changes.",
                    "Forget",
                )
                .inline(),
            ),
            Action::CancelActivity { .. } => ActionMeta::new(
                "Cancel",
                "Stop what Studio is doing to this device.",
                ActionPriority::Secondary,
            ),
            Action::Identify { .. } => ActionMeta::new(
                "Identify again",
                "Ask the board what it is, right now.",
                ActionPriority::Secondary,
            ),
            // No confirmation on purpose: the pick + the one primary verb IS
            // the deliberate gesture (the card ruling), and the boards this
            // face appears on have no LightPlayer install to lose.
            Action::Flash { .. } => ActionMeta::new(
                "Flash firmware",
                "Write LightPlayer firmware for the picked board onto this chip.",
                ActionPriority::Primary,
            ),
            // No confirmation: the empty face's picker IS the deliberate
            // gesture, and a board with nothing on it has nothing to lose.
            // (Pushing OVER a project is M4's banking question, not this
            // face's.)
            Action::Push { .. } => ActionMeta::new(
                "Put it on the board",
                "Send the picked project to this board and start it running.",
                ActionPriority::Primary,
            ),
            Action::ResetBoard { .. } => ActionMeta::new(
                "Reset",
                "Reboot the board (a hardware reset) and identify what starts up.",
                ActionPriority::Secondary,
            ),
            Action::Erase { .. } => ActionMeta::new(
                "Factory reset",
                "Erase the firmware and everything stored on this board.",
                ActionPriority::Tertiary,
            )
            .destructive()
            .with_confirmation(
                ActionConfirmation::new(
                    "Factory reset this board?",
                    "Everything on its flash is erased — firmware, projects, settings. \
                     Its identity lives in silicon and survives; Studio keeps the entry.",
                    "Erase everything",
                )
                .inline(),
            ),
            // Destructive on the BOARD and nowhere else, which is exactly
            // what the confirm has to say: the library copy is a different
            // object and this does not touch it.
            Action::RemoveProject { .. } => ActionMeta::new(
                "Remove project",
                "Stop what this board is running and delete it from the board.",
                ActionPriority::Tertiary,
            )
            .destructive()
            .with_confirmation(
                ActionConfirmation::new(
                    "Remove the project from this board?",
                    "The board stops running it and the project is deleted from the \
                     board's storage. The firmware stays, and your copy in the \
                     library is untouched.",
                    "Remove project",
                )
                .inline(),
            ),
            Action::SetName { .. } => ActionMeta::new(
                "Rename",
                "Change what Studio calls this device.",
                ActionPriority::Secondary,
            ),
            Action::SetAutoconnect { .. } => ActionMeta::new(
                "Connect automatically",
                "Open this device's port whenever it appears.",
                ActionPriority::Tertiary,
            ),
        }
    }

    /// Every device gesture is [`ActionClass::Recovery`].
    ///
    /// Not laziness: a device action owns a port for its duration and its
    /// whole point is to be reachable while something else is stuck — that is
    /// the class the retired `DeviceOp` variants all carried, and it is what
    /// makes Forget work mid-activity from the queue's point of view too. The
    /// model bounds the work itself (deadlines are its timers), so no
    /// quiet-gap budget belongs here.
    fn action_class(&self) -> ActionClass {
        ActionClass::Recovery
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
    use super::*;
    use lpa_devices::DeviceId;

    #[test]
    fn every_action_renders_a_label_and_owns_the_connection() {
        let device = DeviceId(1);
        let link = lpa_devices::LinkId(1);
        for action in [
            Action::AddFromUsb,
            Action::AdoptLink { link },
            Action::DismissLink { link },
            Action::Connect { device },
            Action::Disconnect { device },
            Action::Forget { device },
            Action::CancelActivity { device },
            Action::Identify { device },
            Action::Flash {
                device,
                board_id: "seeed-xiao-esp32c6".to_string(),
                build_id: "esp32c6-4mb".to_string(),
                park_first: false,
            },
            Action::SetName {
                device,
                name: "Kitchen".to_string(),
            },
            Action::SetAutoconnect {
                device,
                enabled: true,
            },
        ] {
            let op = DevicesOp(action.clone());
            assert!(
                !op.default_action_meta().label.is_empty(),
                "{action:?} renders nothing"
            );
            assert_eq!(op.action_class(), ActionClass::Recovery, "{action:?}");
        }
    }

    /// The two irreversible ones ask first. Forget is reachable everywhere by
    /// model design, so the confirm is the only thing standing between a
    /// stuck card and a deleted record.
    #[test]
    fn the_destructive_gestures_carry_a_confirmation() {
        for action in [
            Action::Forget {
                device: DeviceId(1),
            },
            Action::DismissLink {
                link: lpa_devices::LinkId(1),
            },
        ] {
            let meta = DevicesOp(action.clone()).default_action_meta();
            assert!(meta.destructive, "{action:?}");
            assert!(meta.confirmation.is_some(), "{action:?}");
        }
    }
}
