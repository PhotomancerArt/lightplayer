//! The picker's one verb: mint a runtime of a target and power it on (D44).
//!
//! One verb for both kinds (D1). The op carries a [`Backing`] and the
//! controller writes `kind: "sim"` or `kind: "emu"` into the same sidecar
//! through the same catalog op; there is no second creation flow, because
//! there is no second kind of record ([`sim_record`](super::sim_record)).
//! An emu's one extra property — that it comes up **born flashed** (D22) —
//! is not a step here at all: it is what the ordinary `Connect` below does,
//! because the emu transport's power-on hands the worker a manifest URL and
//! the worker fetches, writes and boots.
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

use super::runtime_backing::Backing;

/// Start a runtime of `target` here: mint the record, power it on.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SimCreateOp {
    /// The catalog board id the runtime wears — `lightplayer/desktop` or a
    /// board. The controller normalizes it through
    /// [`ProjectTarget`](crate::app::library::ProjectTarget), so the
    /// Desktop spellings cannot diverge.
    pub target: String,
    /// What to call it. `None` names it after the target and the backing
    /// ([`sim_device_name`]).
    pub name: Option<String>,
    /// Which runtime to make (D1). The picker's row carries it — the emu
    /// row and the sim row of one board differ in exactly this — and it is
    /// on the OP rather than derived from the target because sim-vs-emu is
    /// the user's choice, not a property of the board.
    pub backing: Backing,
}

impl SimCreateOp {
    /// The node id creation gestures target — the devices node, because a
    /// runtime's birth is a Devices-page gesture and its dispatch class is
    /// the same as every other device verb's.
    pub const NODE_ID: &'static str = "studio|device-create";

    /// This op as a dispatchable [`UiAction`](crate::UiAction).
    pub fn action_for(target: impl Into<String>, backing: Backing) -> crate::UiAction {
        Self {
            target: target.into(),
            name: None,
            backing,
        }
        .into_action()
    }

    /// The same, every field as given.
    pub fn into_action(self) -> crate::UiAction {
        crate::UiAction::from_op(crate::ControllerId::new(Self::NODE_ID), self)
    }
}

/// What a runtime minted from the picker is called: the target's display
/// name with the runtime said out loud.
///
/// The name is the ONE thing about a runtime a person owns (D46 — it is a
/// thin record: a name, a target, an identity), and it is renameable from
/// the card's ⋯ menu like any other device's. The backing is in it because
/// three devices of the same board — the one on the desk, the sim in the
/// tab and the emu in the tab — otherwise arrive with the same name.
pub fn sim_device_name(board_id: &str, backing: Backing) -> String {
    format!(
        "{} ({})",
        crate::board_display_name(board_id),
        backing.tag()
    )
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
    fn a_minted_runtime_is_named_after_its_target_and_its_backing() {
        assert_eq!(
            sim_device_name("lightplayer/desktop", Backing::Sim),
            "Desktop (sim)"
        );
        assert_eq!(
            sim_device_name("seeed/xiao-esp32-c6", Backing::Sim),
            "XIAO ESP32-C6 (sim)"
        );
        assert_eq!(
            sim_device_name("seeed/xiao-esp32-c6", Backing::Emu),
            "XIAO ESP32-C6 (emu)"
        );
        assert_ne!(
            sim_device_name("seeed/xiao-esp32-c6", Backing::Emu),
            sim_device_name("seeed/xiao-esp32-c6", Backing::Sim),
            "two runtimes of one board are two devices with two names"
        );
    }

    /// The op carries the target and the backing and nothing else the model
    /// could disagree with, and it dispatches like a device verb.
    #[test]
    fn the_op_carries_the_target_and_the_backing_and_dispatches_as_recovery() {
        for backing in [Backing::Emu, Backing::Sim] {
            let action = SimCreateOp::action_for("seeed/xiao-esp32-c6", backing);
            let op = action
                .op_as::<SimCreateOp>()
                .expect("the picker dispatches a creation op");

            assert_eq!(op.target, "seeed/xiao-esp32-c6");
            assert_eq!(op.name, None);
            assert_eq!(op.backing, backing);
            assert_eq!(op.action_class(), ActionClass::Recovery);
            assert!(!op.default_action_meta().label.is_empty());
        }
    }

    /// Two rows of the same board are two different gestures: the op's own
    /// equality is what the dispatch dedupe reads.
    #[test]
    fn the_two_rows_of_one_board_are_not_the_same_op() {
        assert_ne!(
            SimCreateOp::action_for("seeed/xiao-esp32-c6", Backing::Emu)
                .op_as::<SimCreateOp>()
                .cloned(),
            SimCreateOp::action_for("seeed/xiao-esp32-c6", Backing::Sim)
                .op_as::<SimCreateOp>()
                .cloned()
        );
    }
}
