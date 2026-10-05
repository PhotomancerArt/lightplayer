use crate::{ActionConfirmation, ActionConsequence, ActionEnablement, ActionPriority};

/// Render metadata for a `UiAction`.
///
/// This is the part of an action that a component can display without knowing
/// the concrete controller operation. Keep operation-specific behavior in the
/// operation type and use metadata for labels, help text, icon hints, priority,
/// enablement, and what pressing it costs.
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
    /// What pressing it costs the user (see [`ActionConsequence`]): decides
    /// both how every renderer draws it and whether the app agent may press
    /// it.
    pub consequence: ActionConsequence,
    /// The browser only allows it from a real click (user activation):
    /// `navigator.serial.requestPort()`, `navigator.bluetooth.requestDevice()`.
    /// A platform fact, not a consequence: the button looks plain, but the
    /// app agent hands it to the user instead of pressing it.
    pub needs_user_activation: bool,
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
            consequence: ActionConsequence::Routine,
            needs_user_activation: false,
        }
    }

    /// Set the level outright, for an action whose level depends on state
    /// (a removal that sweeps unsaved edits is Lasting, a clean one is not).
    pub fn with_consequence(mut self, consequence: ActionConsequence) -> Self {
        self.consequence = consequence;
        self
    }

    /// It removes something the user can still get back in Studio (see
    /// [`ActionConsequence::Undoable`]).
    pub fn undoable(mut self) -> Self {
        self.consequence = ActionConsequence::Undoable;
        self
    }

    /// It is gone for good; `copy` says what is lost (see
    /// [`ActionConsequence::Lasting`]).
    pub fn lasting(mut self, copy: ActionConfirmation) -> Self {
        self.consequence = ActionConsequence::Lasting(copy);
        self
    }

    /// The browser only allows it from a real click (the
    /// `needs_user_activation` field).
    pub fn needs_user_activation(mut self) -> Self {
        self.needs_user_activation = true;
        self
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

    /// Whether only the user may press this: what it takes is gone for
    /// good, or the browser needs the user's own click. The app agent
    /// offers such an action as a card instead of dispatching it.
    pub fn needs_user(&self) -> bool {
        self.consequence.arms() || self.needs_user_activation
    }
}
