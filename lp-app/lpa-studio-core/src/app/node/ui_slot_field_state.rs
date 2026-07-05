//! UI state shared by config slot field components.

use crate::UiNodeDirtyState;

/// Interaction and validation state for a config slot field.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiSlotFieldState {
    /// Whether Studio should present the value as editable.
    pub editable: bool,
    /// Edited-state affordance for local value changes.
    pub dirty: UiNodeDirtyState,
    /// Validation error shown near the field when present.
    pub invalid: Option<String>,
    /// True when the slot's policy persistence is transient: edits apply
    /// live to the running project and are **not** written back by save.
    /// M2 styles transient (`live`) dirty differently from persisted dirty.
    pub live: bool,
}

impl UiSlotFieldState {
    /// Clean editable state for ordinary authorable slots.
    pub fn editable() -> Self {
        Self {
            editable: true,
            dirty: UiNodeDirtyState::Clean,
            invalid: None,
            live: false,
        }
    }

    /// Clean read-only state for projected or derived values.
    pub fn readonly() -> Self {
        Self {
            editable: false,
            dirty: UiNodeDirtyState::Clean,
            invalid: None,
            live: false,
        }
    }

    /// Mark the field with an edited-state affordance.
    pub fn with_dirty(mut self, dirty: UiNodeDirtyState) -> Self {
        self.dirty = dirty;
        self
    }

    /// Mark the field invalid with a human-readable reason.
    pub fn with_invalid(mut self, invalid: impl Into<String>) -> Self {
        self.invalid = Some(invalid.into());
        self
    }

    /// Mark whether the field is a live (transient-persistence) control.
    pub fn with_live(mut self, live: bool) -> Self {
        self.live = live;
        self
    }

    /// Returns true when the field has visible state chrome.
    pub fn needs_attention(&self) -> bool {
        self.dirty.needs_attention() || self.invalid.is_some()
    }
}

impl Default for UiSlotFieldState {
    fn default() -> Self {
        Self::editable()
    }
}

#[cfg(test)]
mod tests {
    use crate::{UiNodeDirtyState, UiSlotFieldState};

    #[test]
    fn invalid_state_needs_attention() {
        let state = UiSlotFieldState::editable().with_invalid("expected a finite value");

        assert!(state.needs_attention());
    }

    #[test]
    fn dirty_state_needs_attention() {
        let state = UiSlotFieldState::editable().with_dirty(UiNodeDirtyState::Dirty);

        assert!(state.needs_attention());
    }
}
