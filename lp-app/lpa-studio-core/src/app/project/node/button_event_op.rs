//! Synthetic button-event operation (the runtime command channel's
//! simulate-press consumer).

use core::any::Any;

use lpc_wire::WireButtonEvent;

use crate::{
    ActionClass, ActionMeta, ActionPriority, ControllerOp, PROJECT_EDITOR_ACTION_DEADLINE,
    ProjectNodeAddress,
};

/// Poke a button node's runtime with a SYNTHETIC event, via
/// `WireProjectCommand::NodeCommand` → `ButtonNode` (the button face's
/// simulate-press control). A runtime poke, not an edit: nothing is staged
/// in the overlay, nothing shows in the Save panel, and the event lands on
/// the engine's next frame exactly as a real GPIO transition would.
///
/// Dispatched to `ProjectController::NODE_ID` like [`crate::SlotEditOp`];
/// the controller resolves the button's CURRENT runtime `NodeId` from the
/// stable authored address at dispatch time, so a queued click can never
/// address a stale runtime id across a project reload.
///
/// A sustained hold ([`WireButtonEvent::Press`]) is a REPEATED op, not a
/// long-running one: the face re-sends the same `press_id` on a renewal
/// cadence while held, and the device-side TTL clears the hold on its own
/// if the renewals stop (tab closed, card unmounted). There is therefore no
/// background action class here — every event is the same foreground poke.
#[derive(Clone, Debug, PartialEq)]
pub struct ButtonEventOp {
    /// Stable address of the button node the event is addressed to.
    pub node: ProjectNodeAddress,
    /// The synthetic event: a click, a press (begin/renew), or a release.
    pub event: WireButtonEvent,
}

impl ButtonEventOp {
    /// Human label for the event, used in the action meta and in the
    /// rejection notice ("Couldn't … the button: …").
    fn verb(&self) -> &'static str {
        match self.event {
            WireButtonEvent::Click => "Simulate press",
            WireButtonEvent::Press { .. } => "Hold button",
            WireButtonEvent::Release { .. } => "Release button",
        }
    }
}

impl ControllerOp for ButtonEventOp {
    fn default_action_meta(&self) -> ActionMeta {
        ActionMeta::new(
            self.verb(),
            "Send a synthetic event to this button's runtime now.",
            ActionPriority::Primary,
        )
    }

    fn action_class(&self) -> ActionClass {
        // Same editor foreground class as the playlist activate poke: a
        // press should feel immediate, and a rejection surfaces as a notice.
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
    use super::*;

    fn op(event: WireButtonEvent) -> ButtonEventOp {
        ButtonEventOp {
            node: ProjectNodeAddress::parse("/demo.project/panel.button").unwrap(),
            event,
        }
    }

    #[test]
    fn button_events_are_editor_foreground_class() {
        let click = op(WireButtonEvent::Click);

        assert_eq!(
            click.action_class(),
            ActionClass::Foreground {
                deadline: PROJECT_EDITOR_ACTION_DEADLINE,
            }
        );
        assert_eq!(click.default_action_meta().label, "Simulate press");
        assert_eq!(
            click.default_action_meta().priority,
            ActionPriority::Primary
        );
    }

    #[test]
    fn press_and_release_label_the_hold_they_drive() {
        assert_eq!(
            op(WireButtonEvent::Press {
                press_id: 9,
                ttl_ms: 5000,
            })
            .default_action_meta()
            .label,
            "Hold button"
        );
        assert_eq!(
            op(WireButtonEvent::Release { press_id: 9 })
                .default_action_meta()
                .label,
            "Release button"
        );
    }

    #[test]
    fn press_ids_distinguish_otherwise_identical_ops() {
        // Renewals re-send the SAME op (equal), a new gesture does not —
        // the queue must never coalesce two distinct holds.
        assert_eq!(
            op(WireButtonEvent::Press {
                press_id: 1,
                ttl_ms: 5000
            }),
            op(WireButtonEvent::Press {
                press_id: 1,
                ttl_ms: 5000
            })
        );
        assert_ne!(
            op(WireButtonEvent::Press {
                press_id: 1,
                ttl_ms: 5000
            }),
            op(WireButtonEvent::Press {
                press_id: 2,
                ttl_ms: 5000
            })
        );
    }
}
