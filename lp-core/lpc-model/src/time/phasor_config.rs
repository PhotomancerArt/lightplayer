//! Phasor configuration: the authored shape of a wrapped `[0,1)` timebase read.
//!
//! A [`PhasorConfig`] is *config*, never state. The engine's timebase store
//! keeps the integrator (phase + cycle) keyed by provenance and takes the
//! config fresh with every query, so re-authoring the period changes the rate
//! from that instant on without resetting the phase (continuity, parent D8).
//!
//! [`Waveform`] and [`PhasorConfig::phase_offset`] are **not** applied by the
//! store — its contract is the raw ramp. Evaluators (shader uniform fill and
//! friends) shape the ramp on the way out.
//!
//! Both impl blocks are hand-rolled rather than derived: the `SlotValue`
//! derive can only infer `LpType`s for a fixed scalar whitelist, and a nested
//! string-leaf enum field is not on it. The code below is exactly what the
//! derive would emit for two `f32` fields plus a `String`-typed member — keep
//! it in that shape so a future macro extension can replace it mechanically.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

use crate::{
    FromLpValue, LpType, LpValue, ModelStructMember, SlotMeta, SlotShape, SlotShapeId, SlotValue,
    SlotValueShape, StaticLpType, StaticModelStructMember, StaticSlotMeta, StaticSlotShape,
    StaticSlotShapeDescriptor, StaticSlotValueShape, StaticValueEditorHint, ToLpValue,
    ValueEditorHint, ValueRootError,
};

/// Native shape name for [`Waveform`].
pub const WAVEFORM_SHAPE_NAME: &str = "lp::time::Waveform";

/// Native shape name for [`PhasorConfig`].
pub const PHASOR_CONFIG_SHAPE_NAME: &str = "lp::time::PhasorConfig";

/// Default phasor period in seconds.
pub const DEFAULT_PHASOR_PERIOD_SECONDS: f32 = 4.0;

/// Shape applied to a phasor's raw `[0,1)` ramp on the way out of the engine.
///
/// Always slot-local (settled): a waveform is how one consumer wants to read
/// the phase, never something a channel dictates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Waveform {
    /// Raw wrapped ramp — the phasor's own value, unshaped.
    #[default]
    Ramp,
    Sine,
    Triangle,
    Square,
}

impl Waveform {
    /// Snake-case wire tag.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ramp => "ramp",
            Self::Sine => "sine",
            Self::Triangle => "triangle",
            Self::Square => "square",
        }
    }

    /// Parse a snake-case wire tag.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "ramp" => Some(Self::Ramp),
            "sine" => Some(Self::Sine),
            "triangle" => Some(Self::Triangle),
            "square" => Some(Self::Square),
            _ => None,
        }
    }

    /// Every waveform, in declaration order (pickers, tests).
    #[must_use]
    pub const fn all() -> &'static [Waveform] {
        &[
            Waveform::Ramp,
            Waveform::Sine,
            Waveform::Triangle,
            Waveform::Square,
        ]
    }
}

impl core::fmt::Display for Waveform {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

// --- Waveform: hand-rolled string leaf (impl_string_leaf! precedent in
// `nodes/shader/shader_slot_def.rs`; that macro is private to its module).

impl Serialize for Waveform {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Waveform {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value)
            .ok_or_else(|| serde::de::Error::custom(alloc::format!("unknown waveform {value:?}")))
    }
}

// On the wire a waveform is its snake-case tag — the same string serde
// writes above — so the schema is a closed string enum.
#[cfg(feature = "schema-gen")]
impl schemars::JsonSchema for Waveform {
    fn schema_name() -> alloc::borrow::Cow<'static, str> {
        "Waveform".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let tags: Vec<&'static str> = Waveform::all().iter().map(|w| w.as_str()).collect();
        schemars::json_schema!({
            "description": "Phasor output shaping, as its snake-case tag.",
            "type": "string",
            "enum": tags,
        })
    }
}

impl ToLpValue for Waveform {
    fn to_lp_value(&self) -> LpValue {
        LpValue::String(self.as_str().to_string())
    }
}

impl FromLpValue for Waveform {
    fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
        match value {
            LpValue::String(value) => {
                Self::parse(value).ok_or_else(|| ValueRootError::new("expected waveform"))
            }
            other => Err(ValueRootError::new(alloc::format!(
                "expected String, got {other:?}"
            ))),
        }
    }
}

impl SlotValue for Waveform {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name(WAVEFORM_SHAPE_NAME);
    const STATIC_VALUE_SHAPE_DESCRIPTOR: Option<StaticSlotValueShape> = Some(
        StaticSlotValueShape::new(<Waveform as SlotValue>::SHAPE_ID, StaticLpType::String),
    );

    fn value_shape() -> SlotValueShape {
        SlotValueShape {
            id: <Waveform as SlotValue>::SHAPE_ID,
            ty: LpType::String,
            meta: SlotMeta::empty(),
            editor: ValueEditorHint::Plain,
        }
    }
}

impl StaticSlotShape for Waveform {
    const SHAPE_ID: SlotShapeId = <Self as SlotValue>::SHAPE_ID;
    const STATIC_SLOT_SHAPE_DESCRIPTOR: Option<&'static StaticSlotShapeDescriptor> =
        Some(&StaticSlotShapeDescriptor::Value {
            shape: StaticSlotValueShape::new(
                <Waveform as SlotValue>::SHAPE_ID,
                StaticLpType::String,
            ),
        });

    fn slot_shape() -> SlotShape {
        SlotShape::leaf(<Self as SlotValue>::value_shape())
    }

    fn shape_name() -> Option<&'static str> {
        Some(WAVEFORM_SHAPE_NAME)
    }
}

/// How one consumer wants a timebase read as a wrapped cycle position.
///
/// `period_seconds == 0.0` means **frozen**: the phasor holds whatever phase
/// it had (rate 0), it does not reset. Negative periods are treated as frozen
/// too rather than silently running backwards.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PhasorConfig {
    /// Seconds for one full `[0,1)` cycle. `0.0` = frozen.
    pub period_seconds: f32,
    /// Output shaping applied by the evaluator, never by the store.
    pub waveform: Waveform,
    /// Added to the wrapped phase at evaluation, then re-wrapped.
    pub phase_offset: f32,
}

impl PhasorConfig {
    /// A config with `period_seconds` and otherwise-default shaping.
    #[must_use]
    pub fn with_period(period_seconds: f32) -> Self {
        Self {
            period_seconds,
            ..Self::default()
        }
    }

    /// Cycles per second — `0.0` when frozen (period `<= 0`, or non-finite).
    ///
    /// The one place the frozen rule is decided; everything downstream just
    /// multiplies by this.
    #[must_use]
    pub fn rate_hz(&self) -> f32 {
        if self.period_seconds.is_finite() && self.period_seconds > 0.0 {
            1.0 / self.period_seconds
        } else {
            0.0
        }
    }

    /// Whether this config holds the phase still.
    #[must_use]
    pub fn is_frozen(&self) -> bool {
        self.rate_hz() == 0.0
    }
}

impl Default for PhasorConfig {
    fn default() -> Self {
        Self {
            period_seconds: DEFAULT_PHASOR_PERIOD_SECONDS,
            waveform: Waveform::Ramp,
            phase_offset: 0.0,
        }
    }
}

// --- PhasorConfig: hand-rolled record leaf (what `#[derive(SlotValue)]`
// emits for named-field structs, with the enum member typed as its
// string-leaf `LpType`).

impl ToLpValue for PhasorConfig {
    fn to_lp_value(&self) -> LpValue {
        LpValue::Struct {
            name: Some("PhasorConfig".to_string()),
            fields: Vec::from([
                (
                    "period_seconds".to_string(),
                    self.period_seconds.to_lp_value(),
                ),
                ("waveform".to_string(), self.waveform.to_lp_value()),
                ("phase_offset".to_string(), self.phase_offset.to_lp_value()),
            ]),
        }
    }
}

impl FromLpValue for PhasorConfig {
    fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
        let LpValue::Struct { name, fields } = value else {
            return Err(ValueRootError::new("expected PhasorConfig struct"));
        };
        if name.as_deref() != Some("PhasorConfig") || fields.len() != 3 {
            return Err(ValueRootError::new("expected PhasorConfig struct"));
        }
        Ok(Self {
            period_seconds: read_field(fields, 0, "period_seconds")?,
            waveform: read_field(fields, 1, "waveform")?,
            phase_offset: read_field(fields, 2, "phase_offset")?,
        })
    }
}

fn read_field<T: FromLpValue>(
    fields: &[(String, LpValue)],
    index: usize,
    name: &str,
) -> Result<T, ValueRootError> {
    match fields.get(index) {
        Some((field_name, value)) if field_name == name => T::from_lp_value(value),
        _ => Err(ValueRootError::new(alloc::format!(
            "expected PhasorConfig.{name}"
        ))),
    }
}

const PHASOR_CONFIG_STATIC_TYPE: StaticLpType = StaticLpType::Struct {
    name: Some("PhasorConfig"),
    fields: &[
        StaticModelStructMember {
            name: "period_seconds",
            ty: StaticLpType::F32,
        },
        StaticModelStructMember {
            name: "waveform",
            ty: StaticLpType::String,
        },
        StaticModelStructMember {
            name: "phase_offset",
            ty: StaticLpType::F32,
        },
    ],
};

fn phasor_config_type() -> LpType {
    LpType::Struct {
        name: Some("PhasorConfig".to_string()),
        fields: Vec::from([
            ModelStructMember {
                name: "period_seconds".to_string(),
                ty: LpType::F32,
            },
            ModelStructMember {
                name: "waveform".to_string(),
                ty: LpType::String,
            },
            ModelStructMember {
                name: "phase_offset".to_string(),
                ty: LpType::F32,
            },
        ]),
    }
}

impl SlotValue for PhasorConfig {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name(PHASOR_CONFIG_SHAPE_NAME);
    const STATIC_VALUE_SHAPE_DESCRIPTOR: Option<StaticSlotValueShape> =
        Some(StaticSlotValueShape {
            id: <PhasorConfig as SlotValue>::SHAPE_ID,
            ty: PHASOR_CONFIG_STATIC_TYPE,
            meta: StaticSlotMeta::EMPTY,
            editor: StaticValueEditorHint::Plain,
        });

    fn value_shape() -> SlotValueShape {
        SlotValueShape {
            id: <PhasorConfig as SlotValue>::SHAPE_ID,
            ty: phasor_config_type(),
            meta: SlotMeta::empty(),
            editor: ValueEditorHint::Plain,
        }
    }
}

impl StaticSlotShape for PhasorConfig {
    const SHAPE_ID: SlotShapeId = <Self as SlotValue>::SHAPE_ID;
    const STATIC_SLOT_SHAPE_DESCRIPTOR: Option<&'static StaticSlotShapeDescriptor> =
        Some(&StaticSlotShapeDescriptor::Value {
            shape: StaticSlotValueShape {
                id: <PhasorConfig as SlotValue>::SHAPE_ID,
                ty: PHASOR_CONFIG_STATIC_TYPE,
                meta: StaticSlotMeta::EMPTY,
                editor: StaticValueEditorHint::Plain,
            },
        });

    fn slot_shape() -> SlotShape {
        SlotShape::leaf(<Self as SlotValue>::value_shape())
    }

    fn shape_name() -> Option<&'static str> {
        Some(PHASOR_CONFIG_SHAPE_NAME)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waveform_tags_round_trip() {
        for &waveform in Waveform::all() {
            assert_eq!(Waveform::parse(waveform.as_str()), Some(waveform));
            assert_eq!(
                Waveform::from_lp_value(&waveform.to_lp_value()),
                Ok(waveform)
            );
        }
        assert_eq!(Waveform::parse("saw"), None);
        assert_eq!(Waveform::default(), Waveform::Ramp);
    }

    #[test]
    fn waveform_serde_uses_snake_case_strings() {
        let json = serde_json::to_string(&Waveform::Triangle).unwrap();

        assert_eq!(json, "\"triangle\"");
        assert_eq!(
            serde_json::from_str::<Waveform>("\"square\"").unwrap(),
            Waveform::Square
        );
        assert!(serde_json::from_str::<Waveform>("\"saw\"").is_err());
    }

    #[test]
    fn phasor_config_defaults_to_a_four_second_ramp() {
        let config = PhasorConfig::default();

        assert_eq!(config.period_seconds, 4.0);
        assert_eq!(config.waveform, Waveform::Ramp);
        assert_eq!(config.phase_offset, 0.0);
        assert_eq!(config.rate_hz(), 0.25);
        assert!(!config.is_frozen());
    }

    #[test]
    fn zero_and_negative_periods_are_frozen() {
        assert!(PhasorConfig::with_period(0.0).is_frozen());
        assert!(PhasorConfig::with_period(-1.0).is_frozen());
        assert!(PhasorConfig::with_period(f32::NAN).is_frozen());
        assert_eq!(PhasorConfig::with_period(0.0).rate_hz(), 0.0);
    }

    #[test]
    fn phasor_config_round_trips_through_lp_value() {
        let config = PhasorConfig {
            period_seconds: 2.5,
            waveform: Waveform::Sine,
            phase_offset: 0.25,
        };

        assert_eq!(
            PhasorConfig::from_lp_value(&config.to_lp_value()).unwrap(),
            config
        );
    }

    #[test]
    fn phasor_config_rejects_a_foreign_struct() {
        let wrong = LpValue::Struct {
            name: Some("Other".to_string()),
            fields: Vec::new(),
        };

        assert!(PhasorConfig::from_lp_value(&wrong).is_err());
        assert!(PhasorConfig::from_lp_value(&LpValue::F32(1.0)).is_err());
    }

    #[test]
    fn phasor_config_serde_uses_snake_case_fields() {
        let json = serde_json::to_string(&PhasorConfig::default()).unwrap();

        assert!(json.contains("\"period_seconds\""), "{json}");
        assert!(json.contains("\"phase_offset\""), "{json}");
        assert!(json.contains("\"waveform\":\"ramp\""), "{json}");

        let parsed: PhasorConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, PhasorConfig::default());
    }

    #[test]
    fn static_and_dynamic_phasor_shapes_agree() {
        let dynamic = <PhasorConfig as SlotValue>::value_shape();
        let static_shape =
            <PhasorConfig as SlotValue>::STATIC_VALUE_SHAPE_DESCRIPTOR.expect("static descriptor");

        assert_eq!(static_shape.to_owned_value_shape(), dynamic);
        assert_eq!(
            dynamic.id,
            SlotShapeId::from_static_name(PHASOR_CONFIG_SHAPE_NAME)
        );

        let waveform_dynamic = <Waveform as SlotValue>::value_shape();
        let waveform_static =
            <Waveform as SlotValue>::STATIC_VALUE_SHAPE_DESCRIPTOR.expect("static descriptor");
        assert_eq!(waveform_static.to_owned_value_shape(), waveform_dynamic);
    }
}
