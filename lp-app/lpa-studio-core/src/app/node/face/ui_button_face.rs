//! The button card's permanent face.

use lpc_wire::WireButtonEvent;

use crate::{ButtonEventOp, ControllerId, ProjectController, ProjectNodeAddress, UiAction};

/// Permanent face for a button node card.
///
/// The face is one affordance — simulate a press from the Studio, so a show
/// can be rehearsed (and a binding proven) without reaching for the physical
/// button. The endpoint and message id ride along as the button's identity;
/// everything else stays in the advanced drawer's slot view.
///
/// The face does NOT carry a ready-made action, because a hold's `press_id`
/// is minted per gesture in the view layer. It carries the ADDRESS and the
/// three constructors below, so the op type and its controller routing stay
/// in core while the timing (press window, renewal cadence) stays where the
/// pointer events are.
#[derive(Clone, Debug, PartialEq)]
pub struct UiButtonFace {
    /// Stable address of the button node the simulate-press control pokes.
    pub node: ProjectNodeAddress,
    /// Authored hardware endpoint (`button:gpio:D9`), when the def's row
    /// resolved — the button's physical identity, shown beside the control.
    pub endpoint: Option<String>,
    /// Authored stable message id the button publishes under, when the
    /// def's row resolved.
    pub id: Option<u32>,
}

impl UiButtonFace {
    /// A tap: the minimal down-then-up pair, no hold to manage.
    pub fn click_action(&self) -> UiAction {
        self.action(WireButtonEvent::Click)
    }

    /// Begin — or RENEW — a sustained hold. Renewals re-send this same
    /// action (same `press_id`); the runtime auto-releases at `ttl_ms` if
    /// they stop, which is what makes an unmounted card safe.
    pub fn press_action(&self, press_id: u32, ttl_ms: u32) -> UiAction {
        self.action(WireButtonEvent::Press { press_id, ttl_ms })
    }

    /// End the hold started with `press_id`.
    pub fn release_action(&self, press_id: u32) -> UiAction {
        self.action(WireButtonEvent::Release { press_id })
    }

    fn action(&self, event: WireButtonEvent) -> UiAction {
        UiAction::from_op(
            ControllerId::new(ProjectController::NODE_ID),
            ButtonEventOp {
                node: self.node.clone(),
                event,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face() -> UiButtonFace {
        UiButtonFace {
            node: ProjectNodeAddress::parse("/demo.project/panel.button").unwrap(),
            endpoint: Some("button:gpio:D9".to_string()),
            id: Some(1),
        }
    }

    #[test]
    fn every_event_targets_the_project_controller_with_the_faces_address() {
        let face = face();

        for action in [
            face.click_action(),
            face.press_action(7, 5000),
            face.release_action(7),
        ] {
            assert!(action.is_for_node(ProjectController::NODE_ID));
            let op = action
                .op_as::<ButtonEventOp>()
                .expect("a button event op rides the action");
            assert_eq!(op.node, face.node);
        }
    }

    #[test]
    fn a_renewal_rebuilds_the_identical_press() {
        let face = face();
        assert_eq!(
            face.press_action(7, 5000).op_as::<ButtonEventOp>().unwrap(),
            face.press_action(7, 5000).op_as::<ButtonEventOp>().unwrap(),
        );
        assert_eq!(
            face.press_action(7, 5000)
                .op_as::<ButtonEventOp>()
                .unwrap()
                .event,
            WireButtonEvent::Press {
                press_id: 7,
                ttl_ms: 5000,
            },
        );
    }
}
