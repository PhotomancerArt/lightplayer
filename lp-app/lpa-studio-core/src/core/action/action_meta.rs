use crate::{ActionConfirmation, ActionEnablement, ActionPriority};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionMeta {
    pub label: String,
    pub short_label: Option<String>,
    pub summary: String,
    pub icon: Option<String>,
    pub priority: ActionPriority,
    pub enablement: ActionEnablement,
    pub confirmation: Option<ActionConfirmation>,
}

impl ActionMeta {
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

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = summary.into();
        self
    }

    pub fn with_short_label(mut self, short_label: impl Into<String>) -> Self {
        self.short_label = Some(short_label.into());
        self
    }

    pub fn with_icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
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
