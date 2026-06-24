use crate::{ActionConfirmation, ActionEnablement, ActionPriority};

/// Render metadata for a `UiAction`.
///
/// This is the part of an action that a component can display without knowing
/// the concrete controller operation. Keep operation-specific behavior in the
/// operation type and use metadata for labels, help text, icon hints, priority,
/// enablement, and confirmation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionMeta {
    /// Primary visible label.
    pub label: String,
    /// Optional compact label for constrained layouts.
    pub short_label: Option<String>,
    /// Help text or tooltip copy.
    pub summary: String,
    /// Optional icon token understood by the renderer.
    pub icon: Option<String>,
    /// Visual hierarchy for the action.
    pub priority: ActionPriority,
    /// Whether the action can currently be invoked.
    pub enablement: ActionEnablement,
    /// Optional confirmation required before dispatch.
    pub confirmation: Option<ActionConfirmation>,
}

impl ActionMeta {
    /// Create metadata for an enabled action.
    pub fn new(
        label: impl Into<String>,
        summary: impl Into<String>,
        priority: ActionPriority,
    ) -> Self {
        Self {
            label: label.into(),
            short_label: None,
            summary: summary.into(),
            icon: None,
            priority,
            enablement: ActionEnablement::Enabled,
            confirmation: None,
        }
    }

    /// Override the primary visible label.
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    /// Override the help text or tooltip copy.
    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = summary.into();
        self
    }

    /// Add a compact label for constrained layouts.
    pub fn with_short_label(mut self, short_label: impl Into<String>) -> Self {
        self.short_label = Some(short_label.into());
        self
    }

    /// Attach an icon token.
    pub fn with_icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    /// Mark the action as visible but not invokable.
    pub fn disabled(mut self, reason: impl Into<String>) -> Self {
        self.enablement = ActionEnablement::Disabled {
            reason: reason.into(),
        };
        self
    }

    /// Attach confirmation copy to require approval before dispatch.
    pub fn with_confirmation(mut self, confirmation: ActionConfirmation) -> Self {
        self.confirmation = Some(confirmation);
        self
    }
}
