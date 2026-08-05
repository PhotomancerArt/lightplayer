//! The [`GradientConfig`] value: how one consumer wants a palette read.
//!
//! The second *declared-config* kind after [`crate::PhasorConfig`], and it
//! follows the same rule: **config, never state**. A cycle's position is a
//! pure function of a phasor read at fill time, so re-authoring
//! `step_seconds` changes the rate from that instant on without resetting
//! anything.
//!
//! # Cycle semantics
//!
//! A cycle is **one full-cycle phasor**, not one phasor per entry: its period
//! is `set.len() × step_seconds` ([`GradientConfig::full_cycle_seconds`]) and
//! both the entry index and the cross-fade blend are pure functions of that
//! single wrapped φ. `fade_seconds` is the overlap at each hand-off, carved
//! out of the step it precedes.
//!
//! `step_seconds <= 0` (or non-finite) means **frozen**: the cycle holds
//! whichever entry φ is on, it does not reset and it does not run backwards.
//! That is the same rule [`crate::PhasorConfig::rate_hz`] states for periods,
//! and [`GradientConfig::is_frozen`] is the one place it lives here.
//!
//! Storage is the flattened fixed-shape recipe described in the [module
//! docs](super) — [`crate::LpValue`] has no union, so both variants share one
//! struct and the `kind` tag selects how `count` and the timings read.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

use crate::{
    FromLpValue, LpType, LpValue, ModelStructMember, SlotMeta, SlotShape, SlotShapeId, SlotValue,
    SlotValueShape, StaticLpType, StaticModelStructMember, StaticSlotMeta, StaticSlotShape,
    StaticSlotShapeDescriptor, StaticSlotValueShape, StaticValueEditorHint, ToLpValue,
    ValueEditorHint, ValueRootError,
};

use super::gradient::{
    GRADIENT_STATIC_TYPE, Gradient, GradientError, gradient_lp_type, read_field,
};

/// Native shape name for [`GradientConfig`].
pub const GRADIENT_CONFIG_SHAPE_NAME: &str = "lp::color::GradientConfig";

/// Gradients in a cycle's fixed `set` array — the storage size, not the
/// authored size.
///
/// Deliberately *not* [`crate::MAX_GRADIENT_STOPS`]: a cycle of more than
/// about eight palettes is a playlist, and should be authored as one.
pub const MAX_CYCLE_SET: u32 = 8;

/// Gradients below which a cycle is just a static gradient.
pub const MIN_CYCLE_SET: u32 = 2;

/// Wire tag for [`GradientConfig::Static`] in [`LpValue`] storage.
const STATIC_KIND_TAG: &str = "static";

/// Wire tag for [`GradientConfig::Cycle`] in [`LpValue`] storage.
const CYCLE_KIND_TAG: &str = "cycle";

/// How a consumer wants a palette read over time.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GradientConfig {
    /// One gradient, held.
    Static(Gradient),
    /// A timed walk through a set of gradients.
    Cycle {
        /// The gradients, [`MIN_CYCLE_SET`]..=[`MAX_CYCLE_SET`] once
        /// [`GradientConfig::validate`] passes.
        set: Vec<Gradient>,
        /// Seconds each entry holds. `<= 0` or non-finite is frozen.
        step_seconds: f32,
        /// Cross-fade overlap at each hand-off, in seconds.
        fade_seconds: f32,
    },
}

impl GradientConfig {
    /// Whether this config holds one palette still.
    ///
    /// [`GradientConfig::Static`] always does; a cycle does when its step is
    /// non-positive or non-finite. The one place the frozen rule is decided
    /// for palettes — everything downstream asks here.
    #[must_use]
    pub fn is_frozen(&self) -> bool {
        match self {
            Self::Static(_) => true,
            Self::Cycle { step_seconds, .. } => !step_seconds.is_finite() || *step_seconds <= 0.0,
        }
    }

    /// Seconds for one full pass through the set — the period of the single
    /// phasor a cycle reads. `0.0` when frozen.
    #[must_use]
    pub fn full_cycle_seconds(&self) -> f32 {
        match self {
            Self::Cycle {
                set, step_seconds, ..
            } if !self.is_frozen() => set.len() as f32 * *step_seconds,
            _ => 0.0,
        }
    }

    /// The gradients this config can resolve to, authored order.
    #[must_use]
    pub fn gradients(&self) -> &[Gradient] {
        match self {
            Self::Static(gradient) => core::slice::from_ref(gradient),
            Self::Cycle { set, .. } => set,
        }
    }

    /// Check the invariants storage depends on: the set bounds, and every
    /// gradient in it.
    ///
    /// Timings are deliberately unchecked — the frozen rule absorbs a
    /// non-positive or non-finite `step_seconds` rather than rejecting it,
    /// which is what makes "drag the step to zero to hold" an authoring move
    /// instead of a load error.
    pub fn validate(&self) -> Result<(), GradientError> {
        match self {
            Self::Static(gradient) => gradient.validate(),
            Self::Cycle { set, .. } => {
                if set.len() < MIN_CYCLE_SET as usize {
                    return Err(GradientError::TooFewCycleEntries(set.len()));
                }
                if set.len() > MAX_CYCLE_SET as usize {
                    return Err(GradientError::TooManyCycleEntries(set.len()));
                }
                for (index, gradient) in set.iter().enumerate() {
                    gradient
                        .validate()
                        .map_err(|_| GradientError::CycleEntry(index))?;
                }
                Ok(())
            }
        }
    }
}

impl Default for GradientConfig {
    /// The slot default nobody authored: [`Gradient::default`], held.
    fn default() -> Self {
        Self::Static(Gradient::default())
    }
}

// --- GradientConfig: hand-rolled flattened record.
//
// `LpValue` has no union, so both variants write the same five fields and
// `kind` says how to read them: static ⇒ `count = 1`, `set[0]` is the
// gradient, timings are `0.0`; cycle ⇒ `count` in 2..=8.

impl ToLpValue for GradientConfig {
    fn to_lp_value(&self) -> LpValue {
        let (kind, gradients, step_seconds, fade_seconds) = match self {
            Self::Static(gradient) => (STATIC_KIND_TAG, core::slice::from_ref(gradient), 0.0, 0.0),
            Self::Cycle {
                set,
                step_seconds,
                fade_seconds,
            } => (CYCLE_KIND_TAG, set.as_slice(), *step_seconds, *fade_seconds),
        };

        // Count-bounded, not padded (mirrors `Gradient::to_lp_value`):
        // `count` bounds the read, so padding entries carry no information
        // and a fully padded config alone would overflow the 16 KiB
        // project-read frame budget.
        let set: Vec<LpValue> = gradients
            .iter()
            .take(MAX_CYCLE_SET as usize)
            .map(ToLpValue::to_lp_value)
            .collect();

        LpValue::Struct {
            name: Some("GradientConfig".to_string()),
            fields: Vec::from([
                ("kind".to_string(), kind.to_lp_value()),
                ("set".to_string(), LpValue::Array(set)),
                (
                    "count".to_string(),
                    set_count_tag(gradients.len()).to_lp_value(),
                ),
                ("step_seconds".to_string(), step_seconds.to_lp_value()),
                ("fade_seconds".to_string(), fade_seconds.to_lp_value()),
            ]),
        }
    }
}

impl FromLpValue for GradientConfig {
    fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
        let LpValue::Struct { name, fields } = value else {
            return Err(ValueRootError::new("expected GradientConfig struct"));
        };
        if name.as_deref() != Some("GradientConfig") || fields.len() != 5 {
            return Err(ValueRootError::new("expected GradientConfig struct"));
        }

        let kind: String = read_field(fields, 0, "GradientConfig", "kind")?;
        let count: i32 = read_field(fields, 2, "GradientConfig", "count")?;
        let step_seconds: f32 = read_field(fields, 3, "GradientConfig", "step_seconds")?;
        let fade_seconds: f32 = read_field(fields, 4, "GradientConfig", "fade_seconds")?;

        match kind.as_str() {
            STATIC_KIND_TAG => {
                let mut set = read_gradient_set(fields, set_count(count, 1, 1)?)?;
                Ok(Self::Static(set.remove(0)))
            }
            CYCLE_KIND_TAG => Ok(Self::Cycle {
                set: read_gradient_set(
                    fields,
                    set_count(count, MIN_CYCLE_SET as usize, MAX_CYCLE_SET as usize)?,
                )?,
                step_seconds,
                fade_seconds,
            }),
            other => Err(ValueRootError::new(alloc::format!(
                "unknown GradientConfig.kind {other:?}"
            ))),
        }
    }
}

/// Authored set length as its `I32` storage tag, saturating at the bound so a
/// too-long set writes a readable value; [`GradientConfig::validate`] is what
/// rejects it.
fn set_count_tag(count: usize) -> i32 {
    count.min(MAX_CYCLE_SET as usize) as i32
}

fn set_count(tag: i32, min: usize, max: usize) -> Result<usize, ValueRootError> {
    let count = usize::try_from(tag).unwrap_or(usize::MAX);
    if !(min..=max).contains(&count) {
        return Err(ValueRootError::new(alloc::format!(
            "GradientConfig.count must be {min}..={max} for this kind, got {tag}"
        )));
    }
    Ok(count)
}

/// Read the `set` array and keep only the `count` authored entries.
///
/// Accepts any length in `count..=MAX_CYCLE_SET`: the canonical stored form
/// is count-bounded, and the legacy zero-padded form still decodes.
fn read_gradient_set(
    fields: &[(String, LpValue)],
    count: usize,
) -> Result<Vec<Gradient>, ValueRootError> {
    let Some(("set", LpValue::Array(set))) =
        fields.get(1).map(|(name, value)| (name.as_str(), value))
    else {
        return Err(ValueRootError::new("expected GradientConfig.set"));
    };
    if set.len() < count || set.len() > MAX_CYCLE_SET as usize {
        return Err(ValueRootError::new(alloc::format!(
            "GradientConfig.set must hold count..={MAX_CYCLE_SET} entries, got {}",
            set.len()
        )));
    }
    set.iter()
        .take(count)
        .map(Gradient::from_lp_value)
        .collect()
}

const GRADIENT_CONFIG_STATIC_TYPE: StaticLpType = StaticLpType::Struct {
    name: Some("GradientConfig"),
    fields: &[
        StaticModelStructMember {
            name: "kind",
            ty: StaticLpType::String,
        },
        StaticModelStructMember {
            name: "set",
            ty: StaticLpType::Array(&GRADIENT_STATIC_TYPE, MAX_CYCLE_SET as usize),
        },
        StaticModelStructMember {
            name: "count",
            ty: StaticLpType::I32,
        },
        StaticModelStructMember {
            name: "step_seconds",
            ty: StaticLpType::F32,
        },
        StaticModelStructMember {
            name: "fade_seconds",
            ty: StaticLpType::F32,
        },
    ],
};

/// The canonical [`GradientConfig`] storage recipe.
#[must_use]
pub fn gradient_config_lp_type() -> LpType {
    LpType::Struct {
        name: Some("GradientConfig".to_string()),
        fields: Vec::from([
            ModelStructMember {
                name: "kind".to_string(),
                ty: LpType::String,
            },
            ModelStructMember {
                name: "set".to_string(),
                ty: LpType::Array(
                    alloc::boxed::Box::new(gradient_lp_type()),
                    MAX_CYCLE_SET as usize,
                ),
            },
            ModelStructMember {
                name: "count".to_string(),
                ty: LpType::I32,
            },
            ModelStructMember {
                name: "step_seconds".to_string(),
                ty: LpType::F32,
            },
            ModelStructMember {
                name: "fade_seconds".to_string(),
                ty: LpType::F32,
            },
        ]),
    }
}

impl SlotValue for GradientConfig {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name(GRADIENT_CONFIG_SHAPE_NAME);
    const STATIC_VALUE_SHAPE_DESCRIPTOR: Option<StaticSlotValueShape> =
        Some(StaticSlotValueShape {
            id: <GradientConfig as SlotValue>::SHAPE_ID,
            ty: GRADIENT_CONFIG_STATIC_TYPE,
            meta: StaticSlotMeta::EMPTY,
            editor: StaticValueEditorHint::Gradient,
        });

    fn value_shape() -> SlotValueShape {
        SlotValueShape {
            id: <GradientConfig as SlotValue>::SHAPE_ID,
            ty: gradient_config_lp_type(),
            meta: SlotMeta::empty(),
            editor: ValueEditorHint::Gradient,
        }
    }
}

impl StaticSlotShape for GradientConfig {
    const SHAPE_ID: SlotShapeId = <Self as SlotValue>::SHAPE_ID;
    const STATIC_SLOT_SHAPE_DESCRIPTOR: Option<&'static StaticSlotShapeDescriptor> =
        Some(&StaticSlotShapeDescriptor::Value {
            shape: StaticSlotValueShape {
                id: <GradientConfig as SlotValue>::SHAPE_ID,
                ty: GRADIENT_CONFIG_STATIC_TYPE,
                meta: StaticSlotMeta::EMPTY,
                editor: StaticValueEditorHint::Gradient,
            },
        });

    fn slot_shape() -> SlotShape {
        SlotShape::leaf(<Self as SlotValue>::value_shape())
    }

    fn shape_name() -> Option<&'static str> {
        Some(GRADIENT_CONFIG_SHAPE_NAME)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::gradient::{Colorspace, GradientStop, InterpMethod};

    fn swatches(count: usize) -> Vec<Gradient> {
        (0..count)
            .map(|index| Gradient {
                space: Colorspace::Oklab,
                method: InterpMethod::Step,
                stops: Vec::from([
                    GradientStop {
                        at: 0.0,
                        c: [index as f32, 0.0, 0.0],
                    },
                    GradientStop {
                        at: 1.0,
                        c: [0.0, index as f32, 0.0],
                    },
                ]),
            })
            .collect()
    }

    fn cycle(count: usize, step_seconds: f32) -> GradientConfig {
        GradientConfig::Cycle {
            set: swatches(count),
            step_seconds,
            fade_seconds: 0.5,
        }
    }

    #[test]
    fn default_is_the_default_gradient_held() {
        let config = GradientConfig::default();

        assert_eq!(config, GradientConfig::Static(Gradient::default()));
        assert!(config.is_frozen());
        assert_eq!(config.full_cycle_seconds(), 0.0);
        assert_eq!(config.validate(), Ok(()));
    }

    /// The period is one phasor over the whole set, not one per entry.
    #[test]
    fn full_cycle_is_the_set_length_times_the_step() {
        let config = cycle(4, 2.5);

        assert!(!config.is_frozen());
        assert_eq!(config.full_cycle_seconds(), 10.0);
        assert_eq!(config.gradients().len(), 4);
    }

    #[test]
    fn non_positive_and_non_finite_steps_are_frozen() {
        for step in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            let config = cycle(3, step);
            assert!(config.is_frozen(), "step {step} should freeze");
            assert_eq!(config.full_cycle_seconds(), 0.0);
        }
    }

    #[test]
    fn validate_enforces_the_cycle_set_bounds() {
        assert_eq!(cycle(2, 1.0).validate(), Ok(()));
        assert_eq!(
            cycle(MAX_CYCLE_SET as usize, 1.0).validate(),
            Ok(()),
            "8 gradients is the bound, not one past it"
        );
        assert_eq!(
            cycle(1, 1.0).validate(),
            Err(GradientError::TooFewCycleEntries(1))
        );
        assert_eq!(
            cycle(MAX_CYCLE_SET as usize + 1, 1.0).validate(),
            Err(GradientError::TooManyCycleEntries(9))
        );
    }

    #[test]
    fn validate_reports_which_cycle_entry_is_bad() {
        let mut set = swatches(3);
        set[1].stops.truncate(1);

        assert_eq!(
            GradientConfig::Cycle {
                set,
                step_seconds: 1.0,
                fade_seconds: 0.0,
            }
            .validate(),
            Err(GradientError::CycleEntry(1))
        );
    }

    #[test]
    fn both_variants_round_trip_through_lp_value() {
        for config in [
            GradientConfig::default(),
            GradientConfig::Static(swatches(1).remove(0)),
            cycle(2, 1.0),
            cycle(MAX_CYCLE_SET as usize, 0.25),
            cycle(3, 0.0),
        ] {
            assert_eq!(
                GradientConfig::from_lp_value(&config.to_lp_value()).unwrap(),
                config
            );
        }
    }

    /// Static writes `count = 1`, zero timings, and a ONE-entry set — the
    /// stored form is count-bounded (the fixed 8 is the type's maximum, not
    /// the stored length).
    #[test]
    fn static_storage_is_a_one_entry_flattened_struct() {
        let LpValue::Struct { name, fields } = GradientConfig::default().to_lp_value() else {
            panic!("GradientConfig storage must be a Struct");
        };

        assert_eq!(name.as_deref(), Some("GradientConfig"));
        assert_eq!(
            fields[0],
            ("kind".to_string(), LpValue::String("static".to_string()))
        );
        assert_eq!(fields[2], ("count".to_string(), LpValue::I32(1)));
        assert_eq!(fields[3], ("step_seconds".to_string(), LpValue::F32(0.0)));
        assert_eq!(fields[4], ("fade_seconds".to_string(), LpValue::F32(0.0)));

        let LpValue::Array(set) = &fields[1].1 else {
            panic!("set must be an Array");
        };
        assert_eq!(set.len(), 1);
    }

    /// The legacy zero-padded storage form (a full `MAX_CYCLE_SET` array
    /// with `count` bounding the read) still decodes.
    #[test]
    fn storage_accepts_the_legacy_padded_form() {
        let config = cycle(2, 1.0);
        let LpValue::Struct { name, mut fields } = config.to_lp_value() else {
            panic!("GradientConfig storage must be a Struct");
        };
        let LpValue::Array(set) = &mut fields[1].1 else {
            panic!("set must be an Array");
        };
        set.resize(MAX_CYCLE_SET as usize, Gradient::default().to_lp_value());

        assert_eq!(
            GradientConfig::from_lp_value(&LpValue::Struct { name, fields }).unwrap(),
            config
        );
    }

    #[test]
    fn storage_rejects_a_count_the_kind_tag_disallows() {
        let with_count = |kind: &str, count: i32| {
            let LpValue::Struct { name, mut fields } = cycle(2, 1.0).to_lp_value() else {
                unreachable!()
            };
            fields[0].1 = LpValue::String(kind.to_string());
            fields[2].1 = LpValue::I32(count);
            LpValue::Struct { name, fields }
        };

        assert!(GradientConfig::from_lp_value(&with_count("static", 2)).is_err());
        assert!(GradientConfig::from_lp_value(&with_count("cycle", 1)).is_err());
        assert!(GradientConfig::from_lp_value(&with_count("cycle", 9)).is_err());
        assert!(GradientConfig::from_lp_value(&with_count("playlist", 2)).is_err());
        assert!(GradientConfig::from_lp_value(&LpValue::F32(1.0)).is_err());
    }

    #[test]
    fn serde_is_an_externally_tagged_snake_case_enum() {
        let json = serde_json::to_string(&GradientConfig::default()).unwrap();
        assert!(json.starts_with("{\"static\":"), "{json}");

        let json = serde_json::to_string(&cycle(2, 1.5)).unwrap();
        assert!(json.starts_with("{\"cycle\":"), "{json}");
        assert!(json.contains("\"step_seconds\":1.5"), "{json}");
        assert!(json.contains("\"fade_seconds\":0.5"), "{json}");

        let parsed: GradientConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, cycle(2, 1.5));
    }

    #[test]
    fn static_and_dynamic_gradient_config_shapes_agree() {
        let dynamic = <GradientConfig as SlotValue>::value_shape();
        let static_shape = <GradientConfig as SlotValue>::STATIC_VALUE_SHAPE_DESCRIPTOR
            .expect("static descriptor");

        assert_eq!(static_shape.to_owned_value_shape(), dynamic);
        assert_eq!(
            dynamic.id,
            SlotShapeId::from_static_name(GRADIENT_CONFIG_SHAPE_NAME)
        );
    }
}
