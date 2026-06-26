//! Stable popup sections and summary affordances for config slots.

use super::{ui_slot_shape::UiSlotShape, ui_slot_unit::UiSlotUnit};

/// Compact row-level summary emitted by a slot aspect.
///
/// The enum order is intentional: later variants are more important and win
/// the visible row treatment when multiple aspects provide affordances.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum UiSlotAffordance {
    /// Quiet fallback when no aspect needs special treatment.
    Info,
    /// The slot is currently being written or refreshed.
    Saving,
    /// The slot value comes from a binding.
    Bound,
    /// The slot has a local edit that has not been saved.
    Edited,
    /// The slot value violates validation rules.
    Invalid,
    /// The slot has an operation, projection, or write failure.
    Error,
}

/// The stable categories shown in a slot detail popup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiSlotAspectKind {
    /// Optional inclusion state for a slot whose value may be absent.
    Optionality,
    /// Validation rules and validation results.
    Validation,
    /// Local edit and persistence state.
    EditState,
    /// Direct value, binding, or unset source state.
    Binding,
    /// Slot value family and type metadata.
    TypeInfo,
}

/// A stable popup section for one slot concern.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiSlotAspect {
    /// Stable aspect category.
    pub kind: UiSlotAspectKind,
    /// Human-readable section title.
    pub title: String,
    /// Rows of detail shown inside the popup section.
    pub rows: Vec<UiSlotAspectRow>,
    /// Optional row-level visual summary provided by this aspect.
    pub affordance: Option<UiSlotAffordance>,
}

impl UiSlotAspect {
    /// Create an aspect section.
    pub fn new(kind: UiSlotAspectKind, title: impl Into<String>) -> Self {
        Self {
            kind,
            title: title.into(),
            rows: Vec::new(),
            affordance: None,
        }
    }

    /// Add a detail row.
    pub fn with_row(mut self, row: UiSlotAspectRow) -> Self {
        self.rows.push(row);
        self
    }

    /// Mark the aspect with a row-level affordance.
    pub fn with_affordance(mut self, affordance: UiSlotAffordance) -> Self {
        self.affordance = Some(affordance);
        self
    }
}

/// One label/value line inside a slot aspect section.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiSlotAspectRow {
    /// Compact row label.
    pub label: String,
    /// Main row value.
    pub value: String,
    /// Optional supporting detail.
    pub detail: Option<String>,
    /// Typed shape metadata for rich renderers.
    pub shape: Option<UiSlotShape>,
    /// Typed unit metadata for rich renderers.
    pub unit: Option<UiSlotUnit>,
}

impl UiSlotAspectRow {
    /// Create a detail row.
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            detail: None,
            shape: None,
            unit: None,
        }
    }

    /// Create a typed shape detail row.
    pub fn shape(shape: UiSlotShape) -> Self {
        let value = shape.summary_label();
        let detail = shape.summary_detail();
        Self {
            label: "Shape".to_string(),
            value,
            detail,
            shape: Some(shape),
            unit: None,
        }
    }

    /// Create a typed unit detail row.
    pub fn unit(unit: UiSlotUnit) -> Self {
        Self {
            label: "Unit".to_string(),
            value: unit.long.clone(),
            detail: Some(unit.short.clone()),
            shape: None,
            unit: Some(unit),
        }
    }

    /// Add supporting detail.
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Add typed shape metadata.
    pub fn with_shape(mut self, shape: UiSlotShape) -> Self {
        self.value = shape.summary_label();
        self.detail = shape.summary_detail();
        self.shape = Some(shape);
        self
    }

    /// Add typed unit metadata.
    pub fn with_unit(mut self, unit: UiSlotUnit) -> Self {
        self.value = unit.long.clone();
        self.detail = Some(unit.short.clone());
        self.unit = Some(unit);
        self
    }
}

#[cfg(test)]
mod tests {
    use crate::UiSlotAffordance;

    #[test]
    fn enum_order_is_presentation_priority() {
        assert!(UiSlotAffordance::Error > UiSlotAffordance::Invalid);
        assert!(UiSlotAffordance::Invalid > UiSlotAffordance::Edited);
        assert!(UiSlotAffordance::Edited > UiSlotAffordance::Bound);
        assert!(UiSlotAffordance::Bound > UiSlotAffordance::Info);
    }
}
