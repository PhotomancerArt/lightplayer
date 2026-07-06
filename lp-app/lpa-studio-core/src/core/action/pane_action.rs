//! Contextual pane-header actions as data.

use crate::{ActionPriority, UiAction};

/// One contextual action rendered in a pane header's generic actions slot.
///
/// Pane actions are data produced controller-side: the renderer draws a
/// compact icon control per entry and dispatches the wrapped [`UiAction`]
/// without knowing the concrete operation. The icon token is required
/// (headers render icon buttons); label, summary, priority/emphasis, and
/// enablement are read from the wrapped action's `ActionMeta` so the action
/// stays the single source of that metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiPaneAction {
    /// Icon token understood by the renderer (same vocabulary as
    /// `ActionMeta::icon`, e.g. `"save"`).
    pub icon: String,
    /// The dispatchable controller operation plus its render metadata.
    pub action: UiAction,
}

impl UiPaneAction {
    /// Create a pane action from an icon token and a dispatchable action.
    pub fn new(icon: impl Into<String>, action: UiAction) -> Self {
        Self {
            icon: icon.into(),
            action,
        }
    }

    /// Visible label (tooltip/accessible name), from the action metadata.
    pub fn label(&self) -> &str {
        &self.action.meta().label
    }

    /// Help text or tooltip copy, from the action metadata.
    pub fn summary(&self) -> &str {
        &self.action.meta().summary
    }

    /// True when the action carries primary emphasis.
    pub fn is_primary(&self) -> bool {
        self.action.meta().priority == ActionPriority::Primary
    }

    /// True when the action can currently be invoked.
    pub fn is_enabled(&self) -> bool {
        self.action.meta().enablement.is_enabled()
    }
}

#[cfg(test)]
mod tests {
    use crate::{ControllerId, ProjectOp, UiPaneAction};

    use super::UiAction;

    #[test]
    fn pane_action_exposes_wrapped_action_metadata() {
        let action = UiPaneAction::new(
            "save",
            UiAction::from_op(ControllerId::new("studio|project"), ProjectOp::SaveOverlay),
        );

        assert_eq!(action.icon, "save");
        assert_eq!(action.label(), "Save");
        assert!(action.is_primary());
        assert!(action.is_enabled());
    }

    #[test]
    fn pane_action_reflects_disabled_and_secondary_metadata() {
        let action = UiPaneAction::new(
            "revert",
            UiAction::from_op(
                ControllerId::new("studio|project"),
                ProjectOp::RevertAllEdits,
            )
            .with_label("Revert to saved")
            .disabled("nothing to revert"),
        );

        assert_eq!(action.label(), "Revert to saved");
        assert!(!action.is_primary());
        assert!(!action.is_enabled());
    }
}
