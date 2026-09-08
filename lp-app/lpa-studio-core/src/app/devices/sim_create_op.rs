//! The picker's one verb: mint a sim of a target and power it on (D44).
//!
//! Its own op rather than a [`DevicesOp`](super::DevicesOp) variant, for
//! the reason that file's header gives: `DevicesOp` is a thin envelope over
//! the model's own action vocabulary, and there is no `Action::CreateSim`
//! because creating a device is not something the fold does — the RECORD is
//! made in the library, and the fold meets it as a device like any other at
//! the next settle. Same shape as [`DevicePushOp`](super::DevicePushOp),
//! which is here for the same reason.
//!
//! What the controller does with it is two steps that must not come apart:
//! write the record (registry row + sidecar, one settle) and then raise the
//! ordinary `Connect` at the device it became. A minted sim nobody powered
//! on would appear on the remembered line — a device the user asked for,
//! filed under "not connected".

use core::any::Any;

use crate::{ActionClass, ActionMeta, ActionPriority, ControllerOp};

/// Start a runtime of `target` here: mint the record, power it on.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SimCreateOp {
    /// The catalog board id the sim wears — `lightplayer/desktop` or a
    /// board. The controller normalizes it through
    /// [`ProjectTarget`](crate::app::library::ProjectTarget), so the
    /// Desktop spellings cannot diverge.
    pub target: String,
    /// What to call it. `None` names it after the target
    /// ([`sim_device_name`]).
    pub name: Option<String>,
}

impl SimCreateOp {
    /// The node id creation gestures target — the devices node, because a
    /// sim's birth is a Devices-page gesture and its dispatch class is the
    /// same as every other device verb's.
    pub const NODE_ID: &'static str = "studio|device-create";

    /// This op as a dispatchable [`UiAction`](crate::UiAction).
    pub fn action_for(target: impl Into<String>) -> crate::UiAction {
        Self {
            target: target.into(),
            name: None,
        }
        .into_action()
    }

    /// The same, every field as given.
    pub fn into_action(self) -> crate::UiAction {
        crate::UiAction::from_op(crate::ControllerId::new(Self::NODE_ID), self)
    }
}

/// What a sim minted from the picker is called: the target's display name
/// with the runtime said out loud.
///
/// The name is the ONE thing about a sim a person owns (D46 — it is a thin
/// record: a name, a target, an identity), and it is renameable from the
/// card's ⋯ menu like any other device's. "(sim)" is in it because two
/// devices of the same board — the one on the desk and the one in the tab —
/// otherwise arrive with the same name.
pub fn sim_device_name(board_id: &str) -> String {
    format!("{} (sim)", crate::board_display_name(board_id))
}

impl ControllerOp for SimCreateOp {
    fn default_action_meta(&self) -> ActionMeta {
        ActionMeta::new(
            "Start it here",
            "Make a runtime of this hardware in this tab and power it on.",
            ActionPriority::Primary,
        )
    }

    /// Recovery, like every other device gesture: it ends in a `Connect`,
    /// and it must be reachable while something else on the page is stuck.
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

    #[test]
    fn a_minted_sim_is_named_after_its_target() {
        assert_eq!(sim_device_name("lightplayer/desktop"), "Desktop (sim)");
        assert_eq!(
            sim_device_name("seeed/xiao-esp32-c6"),
            "XIAO ESP32-C6 (sim)"
        );
    }

    /// The op carries the target and nothing else the model could disagree
    /// with, and it dispatches like a device verb.
    #[test]
    fn the_op_carries_the_target_and_dispatches_as_recovery() {
        let action = SimCreateOp::action_for("seeed/xiao-esp32-c6");
        let op = action
            .op_as::<SimCreateOp>()
            .expect("the picker dispatches a creation op");

        assert_eq!(op.target, "seeed/xiao-esp32-c6");
        assert_eq!(op.name, None);
        assert_eq!(op.action_class(), ActionClass::Recovery);
        assert!(!op.default_action_meta().label.is_empty());
    }
}
