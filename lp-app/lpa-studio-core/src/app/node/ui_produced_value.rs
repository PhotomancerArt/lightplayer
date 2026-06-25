//! Produced scalar or structured values.

use crate::{UiNodeDirtyState, UiProducedBinding, UiSlotAspect, UiSlotAspectKind, UiSlotAspectRow};

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

    /// Shared detail aspects for produced value popups.
    pub fn visible_aspects(&self) -> Vec<UiSlotAspect> {
        vec![
            produced_value_info_aspect(self),
            self.binding.output_aspect(),
        ]
    }
}

fn produced_value_info_aspect(value: &UiProducedValue) -> UiSlotAspect {
    let mut display = value.value.clone();
    if let Some(detail) = value.detail.as_ref() {
        display.push(' ');
        display.push_str(detail);
    }

    UiSlotAspect::new(UiSlotAspectKind::TypeInfo, "Info")
        .with_row(UiSlotAspectRow::new("Name", value.label.clone()))
        .with_row(UiSlotAspectRow::new("Shape", "Produced value"))
        .with_row(UiSlotAspectRow::new("Value", display))
}
