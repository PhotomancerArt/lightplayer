//! Typed config slot rows for the Studio node editor.

use crate::{
    ProjectSlotAddress, UiBindingEndpoint, UiNodeDirtyState, UiSlotAffordance, UiSlotAspect,
    UiSlotAspectKind, UiSlotAspectRow, UiSlotAsset, UiSlotFieldState, UiSlotRecord, UiSlotShape,
    UiSlotShapeField, UiSlotSourceState, UiSlotValue,
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
    /// An asset slot rendered by an editor-like expansion.
    Asset(UiSlotAsset),
}

/// Optional inclusion state for a config slot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UiSlotOptionality {
    /// Whether the optional slot currently includes its value.
    pub included: bool,
    /// Whether the current user flow may toggle the inclusion state.
    pub can_toggle: bool,
}

impl UiSlotOptionality {
    /// Create an included optional slot state.
    pub fn included(can_toggle: bool) -> Self {
        Self {
            included: true,
            can_toggle,
        }
    }

    /// Create an excluded optional slot state.
    pub fn excluded(can_toggle: bool) -> Self {
        Self {
            included: false,
            can_toggle,
        }
    }
}

/// A config slot row projected from the LightPlayer slot tree.
#[derive(Clone, Debug, PartialEq)]
pub struct UiConfigSlot {
    /// Stable controller-owned field key.
    pub key: String,
    /// Stable slot address for dispatching edits (`SlotEditOp`) from field
    /// components. `None` for synthetic rows not backed by a project slot.
    pub address: Option<ProjectSlotAddress>,
    /// Human-readable field label.
    pub label: String,
    /// Optional explanatory copy for info popovers and docs.
    pub description: Option<String>,
    /// Optional type, unit, shape, path, or revision detail.
    pub detail: Option<String>,
    /// Optional inclusion state when this row represents an `OptionSlot`.
    pub optionality: Option<UiSlotOptionality>,
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

    /// Create an asset config slot.
    pub fn asset(key: impl Into<String>, label: impl Into<String>, asset: UiSlotAsset) -> Self {
        Self::new(key, label, UiConfigSlotBody::Asset(asset))
    }

    /// Create a config slot with an explicit body.
    pub fn new(key: impl Into<String>, label: impl Into<String>, body: UiConfigSlotBody) -> Self {
        Self {
            key: key.into(),
            address: None,
            label: label.into(),
            description: None,
            detail: None,
            optionality: None,
            source: UiSlotSourceState::Direct,
            body,
            state: UiSlotFieldState::editable(),
            issues: Vec::new(),
            aspects: Vec::new(),
        }
    }

    /// Attach the stable slot address edits should target.
    pub fn with_address(mut self, address: ProjectSlotAddress) -> Self {
        self.address = Some(address);
        self
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

    /// Set optional inclusion metadata.
    pub fn with_optionality(mut self, optionality: UiSlotOptionality) -> Self {
        self.optionality = Some(optionality);
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
    let mut aspects = vec![type_info_aspect(slot)];
    if let Some(optionality) = slot.optionality {
        aspects.push(optionality_aspect(optionality));
    }
    aspects.extend([
        validation_aspect(slot),
        edit_state_aspect(&slot.state),
        binding_aspect(&slot.source),
    ]);
    aspects
}

fn optionality_aspect(optionality: UiSlotOptionality) -> UiSlotAspect {
    let state = if optionality.included {
        "Enabled"
    } else {
        "Disabled"
    };
    UiSlotAspect::new(UiSlotAspectKind::Optionality, "Optional")
        .with_row(UiSlotAspectRow::new(state, ""))
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

    aspect = aspect.with_row(UiSlotAspectRow::shape(body_shape(&slot.body)));

    aspect = match &slot.body {
        UiConfigSlotBody::Value(value) => {
            if let Some(unit) = value.display_unit() {
                aspect.with_row(UiSlotAspectRow::unit(unit))
            } else {
                aspect
            }
        }
        UiConfigSlotBody::Empty | UiConfigSlotBody::Record(_) => aspect,
        UiConfigSlotBody::Asset(asset) => {
            aspect.with_row(UiSlotAspectRow::new("Source", asset.source.clone()))
        }
    };

    aspect
}

fn body_shape(body: &UiConfigSlotBody) -> UiSlotShape {
    match body {
        UiConfigSlotBody::Empty => UiSlotShape::Empty,
        UiConfigSlotBody::Value(value) => UiSlotShape::from_value_kind(&value.kind),
        UiConfigSlotBody::Record(record) => UiSlotShape::Record(
            record
                .fields
                .iter()
                .map(|field| UiSlotShapeField::new(field.label.clone(), body_shape(&field.body)))
                .collect(),
        ),
        UiConfigSlotBody::Asset(asset) => UiSlotShape::Asset(asset.editor_label().to_string()),
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        UiAssetEditorKind, UiBindingEndpoint, UiConfigSlot, UiConfigSlotBody, UiNodeDirtyState,
        UiSlotAffordance, UiSlotAsset, UiSlotFieldState, UiSlotSourceState, UiSlotValue,
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
    fn asset_slot_keeps_editor_data() {
        let slot = UiConfigSlot::asset(
            "shader",
            "Shader",
            UiSlotAsset::new("./shader.glsl", UiAssetEditorKind::Glsl)
                .with_content("void mainImage(out vec4 color, in vec2 uv) {}"),
        );

        let UiConfigSlotBody::Asset(asset) = slot.body else {
            panic!("expected asset slot");
        };
        assert_eq!(asset.source, "./shader.glsl");
        assert_eq!(asset.editor_label(), "GLSL asset");
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
