//! Produced scalar or structured values.

use crate::{UiNodeDirtyState, UiProducedBinding};

/// A non-product output rendered as a compact value box.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiProducedValue {
    /// Human-readable value label.
    pub label: String,
    /// Current formatted value.
    pub value: String,
    /// Optional type, unit, or runtime detail.
    pub detail: Option<String>,
    /// Binding and revision metadata for the value.
    pub binding: UiProducedBinding,
    /// Edited-state affordance for authored produced-value metadata.
    pub dirty: UiNodeDirtyState,
}

impl UiProducedValue {
    /// Create a produced value.
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            detail: None,
            binding: UiProducedBinding::none(),
            dirty: UiNodeDirtyState::Clean,
        }
    }

    /// Add type, unit, or runtime detail.
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}
