//! Typed config slot rows for the Studio node editor.

use crate::{UiSlotFieldState, UiSlotRecord, UiSlotSourceState, UiSlotValue};

/// The renderable body of a config slot row.
#[derive(Clone, Debug, PartialEq)]
pub enum UiConfigSlotBody {
    /// A placeholder or unit slot with no authored value body.
    Empty,
    /// A scalar or vector value rendered by `SlotValueEditor`.
    Value(UiSlotValue),
    /// A structured slot rendered by `SlotRecordEditor`.
    Record(UiSlotRecord),
}

/// A config slot row projected from the LightPlayer slot tree.
#[derive(Clone, Debug, PartialEq)]
pub struct UiConfigSlot {
    /// Stable controller-owned field key.
    pub key: String,
    /// Human-readable field label.
    pub label: String,
    /// Optional explanatory copy for info popovers and docs.
    pub description: Option<String>,
    /// Optional type, unit, shape, path, or revision detail.
    pub detail: Option<String>,
    /// Whether the visible value is direct, bound, or unset.
    pub source: UiSlotSourceState,
    /// Value or record body for the row.
    pub body: UiConfigSlotBody,
    /// Interaction, dirty, and validation state for the field.
    pub state: UiSlotFieldState,
    /// Projection or validation issues associated with this slot.
    pub issues: Vec<String>,
}

impl UiConfigSlot {
    /// Create a scalar or vector config slot.
    pub fn value(key: impl Into<String>, label: impl Into<String>, value: UiSlotValue) -> Self {
        Self::new(key, label, UiConfigSlotBody::Value(value))
    }

    /// Create a record-shaped config slot.
    pub fn record(
        key: impl Into<String>,
        label: impl Into<String>,
        fields: Vec<UiConfigSlot>,
    ) -> Self {
        Self::new(
            key,
            label,
            UiConfigSlotBody::Record(UiSlotRecord::new(fields)),
        )
    }

    /// Create an empty config slot row.
    pub fn empty(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self::new(key, label, UiConfigSlotBody::Empty)
    }

    /// Create a config slot with an explicit body.
    pub fn new(key: impl Into<String>, label: impl Into<String>, body: UiConfigSlotBody) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            description: None,
            detail: None,
            source: UiSlotSourceState::Direct,
            body,
            state: UiSlotFieldState::editable(),
            issues: Vec::new(),
        }
    }

    /// Add an explanatory description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Add compact secondary detail.
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Set the visible value source state.
    pub fn with_source(mut self, source: UiSlotSourceState) -> Self {
        self.source = source;
        self
    }

    /// Set the interaction and validation state.
    pub fn with_state(mut self, state: UiSlotFieldState) -> Self {
        self.state = state;
        self
    }

    /// Add a projection or validation issue.
    pub fn with_issue(mut self, issue: impl Into<String>) -> Self {
        self.issues.push(issue.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use crate::{UiConfigSlot, UiConfigSlotBody, UiSlotValue};

    #[test]
    fn value_slot_keeps_typed_value() {
        let slot = UiConfigSlot::value("fade", "Fade", UiSlotValue::f32(0.35));

        let UiConfigSlotBody::Value(value) = slot.body else {
            panic!("expected value slot");
        };
        assert_eq!(value.display, "0.35");
    }

    #[test]
    fn record_slot_keeps_child_fields() {
        let slot = UiConfigSlot::record(
            "entry",
            "Entry",
            vec![UiConfigSlot::value(
                "duration",
                "Duration",
                UiSlotValue::f32(2.0),
            )],
        );

        let UiConfigSlotBody::Record(record) = slot.body else {
            panic!("expected record slot");
        };
        assert_eq!(record.fields[0].label, "Duration");
    }
}
