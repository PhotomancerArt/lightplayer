//! Output test-pattern operation (the runtime command channel's
//! "is this strip alive?" consumer).

use core::any::Any;

use lpc_wire::WireOutputTestPattern;

use crate::{
    ActionClass, ActionMeta, ActionPriority, ControllerOp, PROJECT_EDITOR_ACTION_DEADLINE,
    ProjectNodeAddress,
};

/// Put an output node's runtime into (or out of) test-pattern mode, via
/// `WireProjectCommand::NodeCommand` → `OutputNode` (the output face's
/// test-pattern toggle). A runtime poke, not an edit: nothing is staged in
/// the overlay, nothing shows in the Save panel, and the graph's own frames
/// resume the moment the pattern clears.
///
/// Dispatched to `ProjectController::NODE_ID` like [`crate::SlotEditOp`];
/// the controller resolves the output's CURRENT runtime `NodeId` from the
/// stable authored address at dispatch time, so a queued toggle can never
/// address a stale runtime id across a project reload.
///
/// A sustained pattern is a REPEATED op, not a long-running one: the face
/// re-sends it on a renewal cadence while the toggle is on, and the
/// device-side TTL restores normal output on its own if the renewals stop
/// (tab closed, card unmounted). There is therefore no background action
/// class here — every send is the same foreground poke.
#[derive(Clone, Debug, PartialEq)]
pub struct OutputTestPatternOp {
    /// Stable address of the output node the pattern is addressed to.
    pub node: ProjectNodeAddress,
    /// The pattern to hold (or [`WireOutputTestPattern::Clear`] to end it).
    pub pattern: WireOutputTestPattern,
    /// Auto-expiry for a sustained pattern, in milliseconds. Ignored by the
    /// runtime for `Clear`.
    pub ttl_ms: u32,
}

impl OutputTestPatternOp {
    /// Human label, used in the action meta and the rejection notice.
    fn verb(&self) -> &'static str {
        match self.pattern {
            WireOutputTestPattern::Clear => "Clear test pattern",
            WireOutputTestPattern::Solid { .. } => "Test pattern",
        }
    }
}

impl ControllerOp for OutputTestPatternOp {
    fn default_action_meta(&self) -> ActionMeta {
        ActionMeta::new(
            self.verb(),
            "Drive this output with a diagnostic pattern instead of the graph.",
            ActionPriority::Primary,
        )
    }

    fn action_class(&self) -> ActionClass {
        // Same editor foreground class as the playlist activate poke: the
        // toggle should feel immediate, and a rejection (never-rendered
        // output) surfaces as a notice.
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

    fn op(pattern: WireOutputTestPattern, ttl_ms: u32) -> OutputTestPatternOp {
        OutputTestPatternOp {
            node: ProjectNodeAddress::parse("/demo.project/strip.output").unwrap(),
            pattern,
            ttl_ms,
        }
    }

    #[test]
    fn test_pattern_is_editor_foreground_class_with_pattern_meta() {
        let solid = op(
            WireOutputTestPattern::Solid {
                r: 255,
                g: 255,
                b: 255,
            },
            2000,
        );

        assert_eq!(
            solid.action_class(),
            ActionClass::Foreground {
                deadline: PROJECT_EDITOR_ACTION_DEADLINE,
            }
        );
        assert_eq!(solid.default_action_meta().label, "Test pattern");
        assert_eq!(
            solid.default_action_meta().priority,
            ActionPriority::Primary
        );
    }

    #[test]
    fn clearing_labels_itself_as_the_way_out() {
        assert_eq!(
            op(WireOutputTestPattern::Clear, 0)
                .default_action_meta()
                .label,
            "Clear test pattern"
        );
    }

    #[test]
    fn renewals_are_equal_ops_and_clear_is_not() {
        let solid = || {
            op(
                WireOutputTestPattern::Solid {
                    r: 255,
                    g: 255,
                    b: 255,
                },
                2000,
            )
        };
        assert_eq!(solid(), solid(), "a renewal re-sends the identical op");
        assert_ne!(solid(), op(WireOutputTestPattern::Clear, 0));
    }
}
