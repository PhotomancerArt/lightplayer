use crate::{ActionConfirmation, ActionDescriptor, ActionPriority};

/// A dispatchable action plus metadata needed by UI and agent consumers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AvailableAction<A> {
    pub action: A,
    pub descriptor: ActionDescriptor,
    pub enabled: bool,
    pub priority: ActionPriority,
    pub confirmation: Option<ActionConfirmation>,
}

impl<A> AvailableAction<A> {
    pub fn new(action: A, descriptor: ActionDescriptor) -> Self {
        Self {
            action,
            descriptor,
            enabled: true,
            priority: ActionPriority::Secondary,
            confirmation: None,
        }
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    pub fn primary(mut self) -> Self {
        self.priority = ActionPriority::Primary;
        self
    }

    pub fn tertiary(mut self) -> Self {
        self.priority = ActionPriority::Tertiary;
        self
    }

    pub fn danger(mut self) -> Self {
        self.priority = ActionPriority::Danger;
        self
    }

    pub fn with_confirmation(mut self, confirmation: ActionConfirmation) -> Self {
        self.confirmation = Some(confirmation);
        self
    }

    pub fn map_action<B>(self, f: impl FnOnce(A) -> B) -> AvailableAction<B> {
        AvailableAction {
            action: f(self.action),
            descriptor: self.descriptor,
            enabled: self.enabled,
            priority: self.priority,
            confirmation: self.confirmation,
        }
    }
}
