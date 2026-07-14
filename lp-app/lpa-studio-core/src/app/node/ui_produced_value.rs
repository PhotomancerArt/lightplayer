//! Produced scalar or structured values.

use crate::{
    UiNodeDirtyState, UiProducedBinding, UiSlotAspect, UiSlotAspectKind, UiSlotAspectRow,
    UiSlotShape, UiSlotUnit,
};

/// A non-product output rendered as a compact value box.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiProducedValue {
    /// Human-readable value label.
    pub label: String,
    /// Current formatted value.
    pub value: String,
    /// Optional type, unit, or runtime detail.
    pub detail: Option<String>,
    /// Structured unit metadata for value presentation.
    pub unit: Option<UiSlotUnit>,
    /// Binding and revision metadata for the value.
    pub binding: UiProducedBinding,
    /// Edited-state affordance for authored produced-value metadata.
    pub dirty: UiNodeDirtyState,
    /// Binding authoring surface when this value is bindable (M4).
    pub authoring: Option<crate::UiBindingAuthoring>,
}

impl UiProducedValue {
    /// Create a produced value.
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            detail: None,
            unit: None,
            binding: UiProducedBinding::none(),
            dirty: UiNodeDirtyState::Clean,
            authoring: None,
        }
    }

    /// Add type, unit, or runtime detail.
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Add structured unit metadata.
    pub fn with_unit(mut self, unit: UiSlotUnit) -> Self {
        self.unit = Some(unit);
        self
    }

    /// Return structured unit metadata, recognizing legacy detail labels.
    pub fn display_unit(&self) -> Option<UiSlotUnit> {
        self.unit.clone().or_else(|| {
            self.detail
                .as_deref()
                .and_then(UiSlotUnit::from_known_label)
        })
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
    if let Some(unit) = value.display_unit() {
        display.push(' ');
        display.push_str(&unit.short);
    }

    let mut aspect = UiSlotAspect::new(UiSlotAspectKind::TypeInfo, "Info")
        .with_row(UiSlotAspectRow::new("Name", value.label.clone()))
        .with_row(UiSlotAspectRow::shape(UiSlotShape::ProducedValue))
        .with_row(UiSlotAspectRow::new("Value", display));

    if let Some(unit) = value.display_unit() {
        aspect = aspect.with_row(UiSlotAspectRow::unit(unit));
    }

    aspect
}
