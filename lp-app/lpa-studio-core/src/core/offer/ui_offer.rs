//! [`UiOffer`]: one verb the user can press, at its path.

use crate::{ActionConsequence, ActionPriority, OfferPath, UiAction};

/// One verb the user can press, addressed by a stable [`OfferPath`].
///
/// The icon is required (header surfaces render icon buttons). Everything
/// else a renderer or the agent needs (label, summary, emphasis,
/// enablement, consequence) is read from the wrapped action's
/// [`crate::ActionMeta`], so the action stays the one source of that
/// metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiOffer {
    /// Where the offer lives: `project/save`,
    /// `project/demo.module/orbit.shader/remove`.
    pub path: OfferPath,
    /// The dispatchable controller operation plus its render metadata.
    pub action: UiAction,
    /// Icon token understood by the renderer (same vocabulary as
    /// `ActionMeta::icon`, e.g. `"save"`).
    pub icon: String,
}

impl UiOffer {
    /// An offer at `path`, drawn with `icon`, that dispatches `action`.
    pub fn new(path: OfferPath, icon: impl Into<String>, action: UiAction) -> Self {
        Self {
            path,
            action,
            icon: icon.into(),
        }
    }

    /// Visible label (tooltip and accessible name).
    pub fn label(&self) -> &str {
        &self.action.meta().label
    }

    /// Help text or tooltip copy.
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

    /// What pressing it costs the user.
    pub fn consequence(&self) -> &ActionConsequence {
        &self.action.meta().consequence
    }
}

#[cfg(test)]
mod tests {
    use crate::{ControllerId, OfferPath, ProjectOp, UiAction, UiOffer};

    #[test]
    fn offer_exposes_wrapped_action_metadata() {
        let offer = UiOffer::new(
            OfferPath::project().child("save"),
            "save",
            UiAction::from_op(ControllerId::new("studio|project"), ProjectOp::SaveOverlay),
        );

        assert_eq!(offer.icon, "save");
        assert_eq!(offer.path.to_string(), "project/save");
        assert_eq!(offer.label(), "Save");
        assert!(offer.is_primary());
        assert!(offer.is_enabled());
        assert!(offer.consequence().is_routine());
    }

    #[test]
    fn offer_reflects_disabled_and_secondary_metadata() {
        let offer = UiOffer::new(
            OfferPath::project().child("revert"),
            "revert",
            UiAction::from_op(
                ControllerId::new("studio|project"),
                ProjectOp::RevertAllEdits,
            )
            .with_label("Revert to saved")
            .disabled("nothing to revert"),
        );

        assert_eq!(offer.label(), "Revert to saved");
        assert!(!offer.is_primary());
        assert!(!offer.is_enabled());
    }
}
