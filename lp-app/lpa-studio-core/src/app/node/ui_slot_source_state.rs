//! Value-source metadata for config slots.

use crate::UiBindingEndpoint;

/// Where a config slot currently receives its value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UiSlotSourceState {
    /// The value is authored directly on this slot.
    Direct,
    /// The value is provided by a binding endpoint.
    Bound(UiBindingEndpoint),
    /// The slot is unset and has no resolved value.
    Unset,
}

impl UiSlotSourceState {
    /// Returns true when the slot is currently backed by a binding.
    pub fn is_bound(&self) -> bool {
        matches!(self, Self::Bound(_))
    }
}

impl Default for UiSlotSourceState {
    fn default() -> Self {
        Self::Direct
    }
}

#[cfg(test)]
mod tests {
    use crate::{UiBindingEndpoint, UiSlotSourceState};

    #[test]
    fn reports_bound_source() {
        let source = UiSlotSourceState::Bound(UiBindingEndpoint::new("bus:time"));

        assert!(source.is_bound());
    }
}
