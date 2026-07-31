//! The output card's permanent face.

use lpc_wire::WireOutputTestPattern;

use crate::{
    ControllerId, OutputTestPatternOp, ProjectController, ProjectNodeAddress, UiAction,
    UiProducedProduct,
};

/// Permanent face for an output node card.
///
/// The face is one affordance — drive the strip with a diagnostic pattern —
/// answering the first question anyone asks of hardware: are these LEDs
/// wired, addressed, and alive at all? The endpoint rides along as the
/// output's identity; everything else stays in the advanced drawer's slot
/// view.
///
/// Like [`crate::UiButtonFace`] the face carries the ADDRESS plus the two
/// constructors below rather than a ready-made action: the op type and its
/// controller routing stay in core, the renewal cadence stays where the
/// timers are.
#[derive(Clone, Debug, PartialEq)]
pub struct UiOutputFace {
    /// Stable address of the output node the test pattern is aimed at.
    pub node: ProjectNodeAddress,
    /// Authored hardware endpoint (`ws281x:rmt:D10`), when the def's row
    /// resolved — the output's physical identity.
    pub endpoint: Option<String>,
    /// What the output is currently being fed, when the node publishes a
    /// control product to preview.
    pub preview: Option<UiProducedProduct>,
}

impl UiOutputFace {
    /// Start — or RENEW — full-white test output. The runtime restores the
    /// graph's own frames at `ttl_ms` if the renewals stop, which is what
    /// makes an unmounted card (or a closed tab) safe.
    pub fn test_pattern_action(&self, ttl_ms: u32) -> UiAction {
        self.action(
            WireOutputTestPattern::Solid {
                r: 255,
                g: 255,
                b: 255,
            },
            ttl_ms,
        )
    }

    /// End test-pattern mode now, without waiting for the TTL.
    pub fn clear_action(&self) -> UiAction {
        self.action(WireOutputTestPattern::Clear, 0)
    }

    fn action(&self, pattern: WireOutputTestPattern, ttl_ms: u32) -> UiAction {
        UiAction::from_op(
            ControllerId::new(ProjectController::NODE_ID),
            OutputTestPatternOp {
                node: self.node.clone(),
                pattern,
                ttl_ms,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face() -> UiOutputFace {
        UiOutputFace {
            node: ProjectNodeAddress::parse("/demo.project/strip.output").unwrap(),
            endpoint: Some("ws281x:rmt:D10".to_string()),
            preview: None,
        }
    }

    #[test]
    fn both_sends_target_the_project_controller_with_the_faces_address() {
        let face = face();

        for action in [face.test_pattern_action(2000), face.clear_action()] {
            assert!(action.is_for_node(ProjectController::NODE_ID));
            let op = action
                .op_as::<OutputTestPatternOp>()
                .expect("a test-pattern op rides the action");
            assert_eq!(op.node, face.node);
        }
    }

    #[test]
    fn the_pattern_is_full_white_and_clearing_carries_no_ttl() {
        let face = face();

        let on = face
            .test_pattern_action(2000)
            .op_as::<OutputTestPatternOp>()
            .unwrap()
            .clone();
        assert_eq!(
            on.pattern,
            WireOutputTestPattern::Solid {
                r: 255,
                g: 255,
                b: 255,
            }
        );
        assert_eq!(on.ttl_ms, 2000);

        let off = face
            .clear_action()
            .op_as::<OutputTestPatternOp>()
            .unwrap()
            .clone();
        assert_eq!(off.pattern, WireOutputTestPattern::Clear);
        assert_eq!(off.ttl_ms, 0);
    }
}
