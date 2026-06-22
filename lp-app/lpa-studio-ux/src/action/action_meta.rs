use crate::{ActionConfirmation, ActionEnablement, ActionKind, ActionPriority};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionMeta {
    pub kind: ActionKind,
    pub label: String,
    pub summary: String,
    pub priority: ActionPriority,
    pub enablement: ActionEnablement,
    pub confirmation: Option<ActionConfirmation>,
}

impl ActionMeta {
    pub fn new(
        kind: ActionKind,
        label: impl Into<String>,
        summary: impl Into<String>,
        priority: ActionPriority,
    ) -> Self {
        Self {
            kind,
            label: label.into(),
            summary: summary.into(),
            priority,
            enablement: ActionEnablement::Enabled,
            confirmation: None,
        }
    }

    pub fn disabled(mut self, reason: impl Into<String>) -> Self {
        self.enablement = ActionEnablement::Disabled {
            reason: reason.into(),
        };
        self
    }

    pub fn with_confirmation(mut self, confirmation: ActionConfirmation) -> Self {
        self.confirmation = Some(confirmation);
        self
    }
}
