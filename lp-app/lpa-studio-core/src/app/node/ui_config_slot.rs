//! Typed config slot rows for the Studio node editor.

use crate::{
    UiBindingEndpoint, UiNodeDirtyState, UiSlotAffordance, UiSlotAspect, UiSlotAspectKind,
    UiSlotAspectRow, UiSlotFieldState, UiSlotRecord, UiSlotSourceState, UiSlotValue,
};

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
    /// Stable popup sections and row-level presentation affordances.
    pub aspects: Vec<UiSlotAspect>,
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
            aspects: Vec::new(),
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

    /// Add one explicit aspect section.
    pub fn with_aspect(mut self, aspect: UiSlotAspect) -> Self {
        self.aspects.push(aspect);
        self
    }

    /// Set explicit aspect sections.
    pub fn with_aspects(mut self, aspects: Vec<UiSlotAspect>) -> Self {
        self.aspects = aspects;
        self
    }

    /// Return explicit aspects, or transitional aspects derived from legacy fields.
    pub fn visible_aspects(&self) -> Vec<UiSlotAspect> {
        if self.aspects.is_empty() {
            default_aspects(self)
        } else {
            self.aspects.clone()
        }
    }

    /// Return the most important row-level affordance for this slot.
    pub fn primary_affordance(&self) -> UiSlotAffordance {
        self.visible_aspects()
            .iter()
            .filter_map(|aspect| aspect.affordance)
            .max()
            .unwrap_or(UiSlotAffordance::Info)
    }
}

fn default_aspects(slot: &UiConfigSlot) -> Vec<UiSlotAspect> {
    vec![
        type_info_aspect(slot),
        validation_aspect(slot),
        edit_state_aspect(&slot.state),
        binding_aspect(&slot.source),
    ]
}

fn validation_aspect(slot: &UiConfigSlot) -> UiSlotAspect {
    let mut aspect = UiSlotAspect::new(UiSlotAspectKind::Validation, "Validation");
    if let Some(invalid) = slot.state.invalid.as_ref() {
        aspect = aspect
            .with_row(UiSlotAspectRow::new("Invalid", invalid.clone()))
            .with_affordance(UiSlotAffordance::Invalid);
    }
    for issue in &slot.issues {
        aspect = aspect.with_row(UiSlotAspectRow::new("Issue", issue.clone()));
    }
    if !slot.issues.is_empty() {
        aspect = aspect.with_affordance(UiSlotAffordance::Error);
    }
    if aspect.rows.is_empty() {
        aspect = aspect.with_row(UiSlotAspectRow::new("Valid", ""));
    }
    aspect
}

fn edit_state_aspect(state: &UiSlotFieldState) -> UiSlotAspect {
    match state.dirty {
        UiNodeDirtyState::Clean => UiSlotAspect::new(UiSlotAspectKind::EditState, "Edit state")
            .with_row(UiSlotAspectRow::new("No changes", "")),
        UiNodeDirtyState::Dirty => UiSlotAspect::new(UiSlotAspectKind::EditState, "Edit state")
            .with_row(UiSlotAspectRow::new("Edited", "Pending local change."))
            .with_affordance(UiSlotAffordance::Edited),
        UiNodeDirtyState::Saving => UiSlotAspect::new(UiSlotAspectKind::EditState, "Edit state")
            .with_row(UiSlotAspectRow::new("Saving", "Value is being written."))
            .with_affordance(UiSlotAffordance::Saving),
        UiNodeDirtyState::Error => UiSlotAspect::new(UiSlotAspectKind::EditState, "Edit state")
            .with_row(UiSlotAspectRow::new(
                "Write failed",
                "The edited value is still preserved.",
            ))
            .with_affordance(UiSlotAffordance::Error),
    }
}

fn binding_aspect(source: &UiSlotSourceState) -> UiSlotAspect {
    match source {
        UiSlotSourceState::Direct => UiSlotAspect::new(UiSlotAspectKind::Binding, "Binding")
            .with_row(UiSlotAspectRow::new("Unbound", "")),
        UiSlotSourceState::Bound(endpoint) => bound_binding_aspect(endpoint),
        UiSlotSourceState::Unset => UiSlotAspect::new(UiSlotAspectKind::Binding, "Binding")
            .with_row(UiSlotAspectRow::new("Unbound", "")),
    }
}

fn bound_binding_aspect(endpoint: &UiBindingEndpoint) -> UiSlotAspect {
    let mut row = UiSlotAspectRow::new("Bound", endpoint.label.clone());
    if let Some(detail) = endpoint.detail.as_ref() {
        row = row.with_detail(detail.clone());
    }
    UiSlotAspect::new(UiSlotAspectKind::Binding, "Binding")
        .with_row(row)
        .with_affordance(UiSlotAffordance::Bound)
}

fn type_info_aspect(slot: &UiConfigSlot) -> UiSlotAspect {
    let mut aspect = UiSlotAspect::new(UiSlotAspectKind::TypeInfo, "Info")
        .with_row(UiSlotAspectRow::new("Name", slot.key.clone()));

    aspect = match &slot.body {
        UiConfigSlotBody::Empty => aspect.with_row(UiSlotAspectRow::new("Shape", "Empty")),
        UiConfigSlotBody::Value(value) => {
            aspect.with_row(UiSlotAspectRow::new("Shape", value.kind.type_label()))
        }
        UiConfigSlotBody::Record(record) => {
            let detail = if record.fields.len() == 1 {
                "1 field".to_string()
            } else {
                format!("{} fields", record.fields.len())
            };
            aspect.with_row(UiSlotAspectRow::new("Shape", "Record").with_detail(detail))
        }
    };

    aspect
}

#[cfg(test)]
mod tests {
    use crate::{
        UiBindingEndpoint, UiConfigSlot, UiConfigSlotBody, UiNodeDirtyState, UiSlotAffordance,
        UiSlotFieldState, UiSlotSourceState, UiSlotValue,
    };

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

    #[test]
    fn primary_affordance_uses_highest_aspect_affordance() {
        let slot = UiConfigSlot::value("fade", "Fade", UiSlotValue::f32(-1.0))
            .with_source(UiSlotSourceState::Bound(UiBindingEndpoint::new(
                "bus#time.seconds",
            )))
            .with_state(
                UiSlotFieldState::editable()
                    .with_dirty(UiNodeDirtyState::Dirty)
                    .with_invalid("value must be non-negative"),
            );

        assert_eq!(slot.primary_affordance(), UiSlotAffordance::Invalid);
    }

    #[test]
    fn bound_source_provides_bound_affordance() {
        let slot = UiConfigSlot::value("time", "Time", UiSlotValue::f32(3.333)).with_source(
            UiSlotSourceState::Bound(UiBindingEndpoint::new("bus#time.seconds")),
        );

        assert_eq!(slot.primary_affordance(), UiSlotAffordance::Bound);
    }
}
