//! Produced scalar or structured values.

use lpc_model::GradientConfig;

use crate::{
    UiNodeDirtyState, UiProducedBinding, UiSlotAspect, UiSlotAspectKind, UiSlotAspectRow,
    UiSlotShape, UiSlotUnit,
};

/// A non-product output rendered as a compact value box.
///
/// `PartialEq` but not `Eq`: a palette reading carries float stops (as every
/// other float-bearing view DTO does).
#[derive(Clone, Debug, PartialEq)]
pub struct UiProducedValue {
    /// Stable slot-path key of the produced slot (e.g. `active_entry`),
    /// empty for hand-built rows. Kind-face derivations key on this — the
    /// label is presentation and may be humanized/renamed freely.
    pub key: String,
    /// Human-readable value label.
    pub label: String,
    /// Current formatted value. For composite (struct) values this is the
    /// compact type name; the per-field readings ride `fields`.
    pub value: String,
    /// Per-field `(name, formatted value)` rows for composite values; empty
    /// for scalars, which render `value` as the stat hero instead.
    pub fields: Vec<(String, String)>,
    /// The palette this value holds, when it is a gradient record — the
    /// probe row draws strips from it instead of listing `space`/`method`/
    /// `count`/`stops` as struct fields (M4 P2). Same posture as
    /// [`crate::UiBusChannelPreview`] on a bus channel: the picture, not a
    /// debug string.
    pub gradient: Option<GradientConfig>,
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
            key: String::new(),
            fields: Vec::new(),
            gradient: None,
            label: label.into(),
            value: value.into(),
            detail: None,
            unit: None,
            binding: UiProducedBinding::none(),
            dirty: UiNodeDirtyState::Clean,
            authoring: None,
        }
    }

    /// Set the stable produced-slot key.
    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = key.into();
        self
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
    // No live value row: the popup describes the slot (identity, shape,
    // unit, wiring) while the pane hero owns the changing reading —
    // duplicating it here just churns while the popup is open (gate
    // feedback 2026-07-15).
    let mut aspect = UiSlotAspect::new(UiSlotAspectKind::TypeInfo, "Info")
        .with_row(UiSlotAspectRow::new("Name", value.label.clone()))
        .with_row(UiSlotAspectRow::shape(UiSlotShape::ProducedValue));

    if let Some(unit) = value.display_unit() {
        aspect = aspect.with_row(UiSlotAspectRow::unit(unit));
    }

    aspect
}
