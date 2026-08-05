//! Authored panel-visibility hint on a slot declaration.

use alloc::string::{String, ToString};
use serde::{Deserialize, Serialize};

use crate::{
    FromLpValue, LpType, LpValue, SlotMeta, SlotShapeId, SlotValue, SlotValueShape, StaticLpType,
    StaticSlotValueShape, ToLpValue, ValueEditorHint, ValueRootError,
};

/// An additive override on the derived panel-membership rule
/// (ADR 2026-08-03-panel-visibility-is-derived, amended): `Show` promotes
/// the binding materialized from the slot's own `default_bind` to
/// publicity, so the control appears even though the wiring is
/// Default-origin (a fixture's brightness fader). Absent means the derived
/// rule alone decides — authored wiring is public, default wiring is not.
///
/// There is deliberately no `Hide`: suppressing AUTHORED wiring is
/// module-level curation (the deferred authored panel layouts), not a
/// kind-level veto, and a hint that can silently override an author's
/// binding is the deleted `panel: bool` flag growing back.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum PanelHint {
    /// The slot's default-bound channel presents a panel control.
    Show,
}

impl PanelHint {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Show => "show",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "show" => Some(Self::Show),
            _ => None,
        }
    }
}

/// Native shape name for [`PanelHint`] as a slot leaf.
pub const PANEL_HINT_SHAPE_NAME: &str = "slot.leaf.panel_hint";

// The hint is *shape metadata* on a native def (the `#[slot(panel = "show")]`
// attribute), but a shader slot is authored DATA — its fields arrive as JSON
// in `shader.json` — so a shader slot spells the same hint as a value. Both
// spellings answer the same question and
// `lpc_engine::engine::authored_def_slot_panel_hint` reads whichever one the
// def in hand carries.

impl ToLpValue for PanelHint {
    fn to_lp_value(&self) -> LpValue {
        LpValue::String(self.as_str().to_string())
    }
}

impl FromLpValue for PanelHint {
    fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
        match value {
            LpValue::String(value) => {
                Self::parse(value).ok_or_else(|| ValueRootError::new("expected panel hint"))
            }
            other => Err(ValueRootError::new(alloc::format!(
                "expected String, got {other:?}"
            ))),
        }
    }
}

impl SlotValue for PanelHint {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name(PANEL_HINT_SHAPE_NAME);
    const STATIC_VALUE_SHAPE_DESCRIPTOR: Option<StaticSlotValueShape> = Some(
        StaticSlotValueShape::new(<PanelHint as SlotValue>::SHAPE_ID, StaticLpType::String),
    );

    fn value_shape() -> SlotValueShape {
        SlotValueShape {
            id: <PanelHint as SlotValue>::SHAPE_ID,
            ty: LpType::String,
            meta: SlotMeta::empty(),
            editor: ValueEditorHint::Plain,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hint_round_trips_as_its_authored_string() {
        assert_eq!(PanelHint::parse("show"), Some(PanelHint::Show));
        assert_eq!(PanelHint::parse("hide"), None, "there is deliberately no Hide");
        assert_eq!(
            PanelHint::from_lp_value(&PanelHint::Show.to_lp_value()),
            Ok(PanelHint::Show)
        );
        assert!(PanelHint::from_lp_value(&LpValue::I32(1)).is_err());
        assert!(PanelHint::from_lp_value(&LpValue::String(String::from("maybe"))).is_err());
    }
}
