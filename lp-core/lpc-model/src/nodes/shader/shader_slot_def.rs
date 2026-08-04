//! Authored shader slot definitions.
//!
//! A shader slot definition describes the semantic data a shader consumes or
//! produces. It is separate from shader source text and can describe native
//! LightPlayer shapes such as `lp::fluid::Emitter` without copying their field
//! structure into every shader artifact.

use crate::{
    BindingRef, FromLpValue, LpType, LpValue, OptionSlot, PhasorConfig, SlotMeta, SlotShapeId,
    SlotValue, SlotValueShape, Slotted, StaticLpType, StaticSlotValueShape, ToLpValue,
    ValueEditorHint, ValueRootError, ValueSlot,
};
use alloc::string::{String, ToString};
use serde::{Deserialize, Serialize};

use super::ShaderSlotMappingDef;

/// Authored definition for one shader consumed or produced slot.
#[derive(Debug, Clone, PartialEq, Slotted)]
pub struct ShaderSlotDef {
    pub kind: ValueSlot<ShaderSlotKind>,
    pub value: ValueSlot<ShaderValueShapeRef>,
    pub key: OptionSlot<ValueSlot<ShaderMapKeyDef>>,
    /// Timebase shaping for a [`ShaderSlotKind::Phasor`] slot: present iff
    /// the kind is `phasor`, absent for every other kind. The waveform is
    /// always slot-local; the period may instead be driven by a bound
    /// config channel, in which case this is the R6 fallback.
    pub phasor: OptionSlot<ValueSlot<PhasorConfig>>,
    /// Declarative default binding endpoint (`bus:<channel>`), materialized
    /// at load when no authored binding names this slot (ADR 2026-07-09).
    /// Consumed slots source from the channel; produced slots publish to it.
    pub default_bind: OptionSlot<ValueSlot<BindingRef>>,
    pub default: OptionSlot<ValueSlot<f32>>,
    pub min: OptionSlot<ValueSlot<f32>>,
    pub max: OptionSlot<ValueSlot<f32>>,
    /// Quantization for this slot's panel control: gestures snap to whole
    /// multiples of `step`. Integer-shaped uniforms (`i32`/`u32`) step by 1
    /// without authoring it — see [`shader_panel_step`].
    pub step: OptionSlot<ValueSlot<f32>>,
    pub mapping: OptionSlot<ShaderSlotMappingDef>,
    pub label: ValueSlot<String>,
    pub description: ValueSlot<String>,
    /// Display unit suffix rendered near the panel control's value
    /// (e.g. "Hz", "%").
    pub unit: OptionSlot<ValueSlot<String>>,
}

impl ShaderSlotDef {
    /// Attach a declarative default binding endpoint (`bus:<channel>`).
    pub fn with_default_bind(mut self, endpoint: BindingRef) -> Self {
        self.default_bind = OptionSlot::some(ValueSlot::new(endpoint));
        self
    }

    pub fn value_f32(label: &str, description: &str, default: f32, min: Option<f32>) -> Self {
        Self {
            kind: ValueSlot::new(ShaderSlotKind::Value),
            value: ValueSlot::new(ShaderValueShapeRef::builtin("f32")),
            key: OptionSlot::none(),
            phasor: OptionSlot::none(),
            default_bind: OptionSlot::none(),
            default: OptionSlot::some(ValueSlot::new(default)),
            min: min
                .map(ValueSlot::new)
                .map_or_else(OptionSlot::none, OptionSlot::some),
            max: OptionSlot::none(),
            step: OptionSlot::none(),
            mapping: OptionSlot::none(),
            label: ValueSlot::new(String::from(label)),
            description: ValueSlot::new(String::from(description)),
            unit: OptionSlot::none(),
        }
    }

    /// A `phasor` slot: a `float` uniform fed by the scope's timebase as a
    /// wrapped `[0,1)` cycle position, shaped by `config`.
    pub fn phasor(label: &str, description: &str, config: PhasorConfig) -> Self {
        Self {
            kind: ValueSlot::new(ShaderSlotKind::Phasor),
            phasor: OptionSlot::some(ValueSlot::new(config)),
            ..Self::value_f32(label, description, 0.0, None)
        }
    }

    /// A `seconds` slot: a `float` uniform fed by the scope's timebase as
    /// unbounded effective seconds.
    pub fn seconds(label: &str, description: &str) -> Self {
        Self {
            kind: ValueSlot::new(ShaderSlotKind::Seconds),
            ..Self::value_f32(label, description, 0.0, None)
        }
    }

    /// The phasor config this slot evaluates against when nothing drives it
    /// from a channel — the authored one, or the default shaping.
    pub fn phasor_config(&self) -> PhasorConfig {
        self.phasor
            .data
            .as_ref()
            .map_or_else(PhasorConfig::default, |config| config.value().clone())
    }

    /// Quantize this slot's panel control to whole multiples of `step`.
    pub fn with_step(mut self, step: f32) -> Self {
        self.step = OptionSlot::some(ValueSlot::new(step));
        self
    }

    /// The step this slot's panel control snaps to, per [`shader_panel_step`].
    pub fn panel_step(&self) -> Option<f32> {
        shader_panel_step(
            self.step.data.as_ref().map(|step| *step.value()),
            self.value.value(),
        )
    }

    pub fn map_u32_native(value: &str, mapping: ShaderSlotMappingDef) -> Self {
        Self {
            kind: ValueSlot::new(ShaderSlotKind::Map),
            value: ValueSlot::new(ShaderValueShapeRef::native(value)),
            key: OptionSlot::some(ValueSlot::new(ShaderMapKeyDef::U32)),
            phasor: OptionSlot::none(),
            default_bind: OptionSlot::none(),
            default: OptionSlot::none(),
            min: OptionSlot::none(),
            max: OptionSlot::none(),
            step: OptionSlot::none(),
            mapping: OptionSlot::some(mapping),
            label: ValueSlot::default(),
            description: ValueSlot::default(),
            unit: OptionSlot::none(),
        }
    }

    pub fn default_value(&self) -> LpValue {
        self.default
            .data
            .as_ref()
            .map_or(LpValue::F32(0.0), |value| LpValue::F32(*value.value()))
    }

    pub fn value_lp_type(&self) -> Option<LpType> {
        self.value.value().as_lp_type()
    }
}

impl Default for ShaderSlotDef {
    fn default() -> Self {
        Self::value_f32("", "", 0.0, None)
    }
}

/// Top-level shader slot shape kind.
///
/// `Phasor` and `Seconds` are the two **timebase** kinds: both declare a
/// plain `float` uniform in GLSL (`value` stays `"f32"`), but their value is
/// evaluated by the host against the scope's time product rather than
/// materialized from the slot's resolved data. A `Phasor` reads the timebase
/// as a wrapped `[0,1)` cycle position shaped by [`ShaderSlotDef::phasor`];
/// a `Seconds` reads it as unbounded effective seconds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ShaderSlotKind {
    #[default]
    Value,
    Map,
    Phasor,
    Seconds,
}

impl ShaderSlotKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Value => "value",
            Self::Map => "map",
            Self::Phasor => "phasor",
            Self::Seconds => "seconds",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "value" => Some(Self::Value),
            "map" => Some(Self::Map),
            "phasor" => Some(Self::Phasor),
            "seconds" => Some(Self::Seconds),
            _ => None,
        }
    }

    /// Whether this kind's uniform is evaluated against the scope's time
    /// product instead of the slot's resolved value.
    #[must_use]
    pub fn is_timebase(self) -> bool {
        matches!(self, Self::Phasor | Self::Seconds)
    }
}

/// Supported map key types for M1 shader slots.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ShaderMapKeyDef {
    #[default]
    U32,
}

impl ShaderMapKeyDef {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::U32 => "u32",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "u32" => Some(Self::U32),
            _ => None,
        }
    }
}

/// Reference to a shader slot value shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShaderValueShapeRef {
    name: String,
}

impl ShaderValueShapeRef {
    pub fn builtin(name: &str) -> Self {
        Self {
            name: String::from(name),
        }
    }

    pub fn native(name: &str) -> Self {
        Self {
            name: String::from(name),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.name
    }

    pub fn is_native(&self) -> bool {
        self.name.starts_with("lp::")
    }

    /// Whether this shape only takes whole values (`i32`/`u32`).
    pub fn is_integer(&self) -> bool {
        matches!(self.as_lp_type(), Some(LpType::I32 | LpType::U32))
    }

    pub fn as_lp_type(&self) -> Option<LpType> {
        match self.name.as_str() {
            "f32" => Some(LpType::F32),
            "u32" => Some(LpType::U32),
            "i32" => Some(LpType::I32),
            "bool" => Some(LpType::Bool),
            "vec2" => Some(LpType::Vec2),
            "vec3" => Some(LpType::Vec3),
            "vec4" => Some(LpType::Vec4),
            _ => None,
        }
    }
}

impl Default for ShaderValueShapeRef {
    fn default() -> Self {
        Self::builtin("f32")
    }
}

/// The step a shader uniform's panel control snaps to: the authored `step`
/// when positive, else 1 for integer-shaped uniforms (`i32`/`u32`) so a
/// "how many" knob lands on whole numbers, else `None` (continuous).
///
/// This is the one rule; both the model ([`ShaderSlotDef::panel_step`]) and
/// the studio's face derivation — which reads authored fields off DTO rows
/// rather than the def itself — call it.
pub fn shader_panel_step(authored: Option<f32>, value: &ShaderValueShapeRef) -> Option<f32> {
    match authored {
        Some(step) if step > 0.0 => Some(step),
        // A non-positive authored step is meaningless; ignore it rather than
        // letting it divide by zero downstream.
        Some(_) => None,
        None => value.is_integer().then_some(1.0),
    }
}

macro_rules! impl_string_leaf {
    ($ty:ty, $shape_id:literal, $expected:literal, $parse:expr, $as_str:expr) => {
        impl Serialize for $ty {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                serializer.serialize_str($as_str(self))
            }
        }

        impl<'de> Deserialize<'de> for $ty {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                $parse(&value)
                    .ok_or_else(|| serde::de::Error::custom(alloc::format!("unknown {value:?}")))
            }
        }

        impl ToLpValue for $ty {
            fn to_lp_value(&self) -> LpValue {
                LpValue::String($as_str(self).to_string())
            }
        }

        impl FromLpValue for $ty {
            fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
                match value {
                    LpValue::String(value) => {
                        $parse(value).ok_or_else(|| ValueRootError::new($expected))
                    }
                    other => Err(ValueRootError::new(alloc::format!(
                        "expected String, got {other:?}"
                    ))),
                }
            }
        }

        impl SlotValue for $ty {
            const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name($shape_id);
            const STATIC_VALUE_SHAPE_DESCRIPTOR: Option<StaticSlotValueShape> = Some(
                StaticSlotValueShape::new(Self::SHAPE_ID, StaticLpType::String),
            );

            fn value_shape() -> SlotValueShape {
                SlotValueShape {
                    id: Self::SHAPE_ID,
                    ty: LpType::String,
                    meta: SlotMeta::empty(),
                    editor: ValueEditorHint::Plain,
                }
            }
        }
    };
}

impl_string_leaf!(
    ShaderSlotKind,
    "slot.leaf.shader_slot_kind",
    "expected shader slot kind",
    ShaderSlotKind::parse,
    |value: &ShaderSlotKind| value.as_str()
);

impl_string_leaf!(
    ShaderMapKeyDef,
    "slot.leaf.shader_map_key",
    "expected shader map key",
    ShaderMapKeyDef::parse,
    |value: &ShaderMapKeyDef| value.as_str()
);

impl Serialize for ShaderValueShapeRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ShaderValueShapeRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        if value.is_empty() {
            return Err(serde::de::Error::custom(
                "shader value shape cannot be empty",
            ));
        }
        Ok(Self { name: value })
    }
}

impl ToLpValue for ShaderValueShapeRef {
    fn to_lp_value(&self) -> LpValue {
        LpValue::String(self.name.clone())
    }
}

impl FromLpValue for ShaderValueShapeRef {
    fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
        match value {
            LpValue::String(value) if !value.is_empty() => Ok(Self {
                name: value.clone(),
            }),
            LpValue::String(_) => Err(ValueRootError::new("shader value shape cannot be empty")),
            other => Err(ValueRootError::new(alloc::format!(
                "expected String, got {other:?}"
            ))),
        }
    }
}

impl SlotValue for ShaderValueShapeRef {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("slot.leaf.shader_value_shape_ref");
    const STATIC_VALUE_SHAPE_DESCRIPTOR: Option<StaticSlotValueShape> = Some(
        StaticSlotValueShape::new(Self::SHAPE_ID, StaticLpType::String),
    );

    fn value_shape() -> SlotValueShape {
        SlotValueShape {
            id: Self::SHAPE_ID,
            ty: LpType::String,
            meta: SlotMeta::empty(),
            editor: ValueEditorHint::Plain,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SlotShapeRegistry, StaticSlotShape};

    #[test]
    fn value_shader_slot_parses_old_param_shape() {
        let slot = read_slot_def(
            r#"{
  "kind": "value",
  "value": "f32",
  "default": 1.0,
  "min": 0.0,
  "label": "Exposure",
  "description": "Output exposure multiplier"
}"#,
        );

        assert_eq!(*slot.kind.value(), ShaderSlotKind::Value);
        assert_eq!(slot.value.value().as_str(), "f32");
        assert_eq!(slot.default_value(), LpValue::F32(1.0));
    }

    #[test]
    fn map_shader_slot_parses_native_value_mapping() {
        let slot = read_slot_def(
            r#"{
  "kind": "map",
  "key": "u32",
  "value": "lp::fluid::Emitter",
  "mapping": { "kind": "sentinel", "len": 4, "key": "id", "empty_key": 0 }
}"#,
        );

        assert_eq!(*slot.kind.value(), ShaderSlotKind::Map);
        assert_eq!(slot.value.value().as_str(), "lp::fluid::Emitter");
        assert!(slot.value.value().is_native());
        assert!(slot.mapping.data.is_some());
    }

    #[test]
    fn phasor_shader_slot_parses_its_config_and_stays_an_f32_uniform() {
        let slot = read_slot_def(
            r#"{
  "kind": "phasor",
  "value": "f32",
  "phasor": { "period_seconds": 2.5, "waveform": "sine", "phase_offset": 0.25 },
  "default_bind": "bus:beat",
  "label": "Wave",
  "description": "Cycle position"
}"#,
        );

        assert_eq!(*slot.kind.value(), ShaderSlotKind::Phasor);
        assert!(slot.kind.value().is_timebase());
        // The GLSL side sees a plain float; the config is def metadata.
        assert_eq!(slot.value_lp_type(), Some(LpType::F32));
        assert_eq!(
            slot.phasor_config(),
            crate::PhasorConfig {
                period_seconds: 2.5,
                waveform: crate::Waveform::Sine,
                phase_offset: 0.25,
            }
        );
    }

    #[test]
    fn seconds_shader_slot_parses_without_a_phasor_config() {
        let slot = read_slot_def(
            r#"{
  "kind": "seconds",
  "value": "f32",
  "label": "Elapsed",
  "description": "Unbounded project seconds"
}"#,
        );

        assert_eq!(*slot.kind.value(), ShaderSlotKind::Seconds);
        assert!(slot.kind.value().is_timebase());
        assert_eq!(slot.value_lp_type(), Some(LpType::F32));
        assert!(slot.phasor.data.is_none());
        // A slot with no authored config still has one to evaluate against.
        assert_eq!(slot.phasor_config(), crate::PhasorConfig::default());
    }

    #[test]
    fn every_kind_tag_round_trips_and_only_timebase_kinds_say_so() {
        for kind in [
            ShaderSlotKind::Value,
            ShaderSlotKind::Map,
            ShaderSlotKind::Phasor,
            ShaderSlotKind::Seconds,
        ] {
            assert_eq!(ShaderSlotKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(
            ShaderSlotKind::parse("phasor"),
            Some(ShaderSlotKind::Phasor)
        );
        assert_eq!(
            ShaderSlotKind::parse("seconds"),
            Some(ShaderSlotKind::Seconds)
        );
        assert_eq!(ShaderSlotKind::parse("lfo"), None);
        assert!(!ShaderSlotKind::Value.is_timebase());
        assert!(!ShaderSlotKind::Map.is_timebase());
    }

    #[test]
    fn integer_shaped_uniforms_step_by_one_without_authoring() {
        let slot = read_slot_def(
            r#"{
  "kind": "value",
  "value": "u32",
  "default": 2,
  "min": 1,
  "max": 4,
  "label": "Count",
  "description": "How many meteors"
}"#,
        );

        assert_eq!(slot.panel_step(), Some(1.0));
    }

    #[test]
    fn authored_step_wins_over_the_shape_default() {
        let slot = read_slot_def(
            r#"{
  "kind": "value",
  "value": "f32",
  "default": 1.0,
  "min": 0.0,
  "max": 4.0,
  "step": 0.25,
  "label": "Speed",
  "description": ""
}"#,
        );

        assert_eq!(slot.panel_step(), Some(0.25));
    }

    #[test]
    fn float_uniforms_stay_continuous_and_bad_steps_are_ignored() {
        assert_eq!(
            ShaderSlotDef::value_f32("Speed", "", 1.0, None).panel_step(),
            None,
            "an unstepped f32 uniform slides continuously"
        );
        assert_eq!(
            ShaderSlotDef::value_f32("Speed", "", 1.0, None)
                .with_step(0.0)
                .panel_step(),
            None,
            "a zero step would divide by zero downstream"
        );
    }

    fn read_slot_def(text: &str) -> ShaderSlotDef {
        let registry = SlotShapeRegistry::default();
        registry
            .read_slot_json(ShaderSlotDef::SHAPE_ID, text)
            .expect("slot")
            .into_any()
            .downcast::<ShaderSlotDef>()
            .map(|def| *def)
            .expect("shader slot def")
    }
}
