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
//! # Pinned
//!
//! `pinned: Some(k)` shows entry `k` alone, as if the set held only that
//! palette: no fade, and φ is ignored. It is the "keep these five, but right
//! now show me this one" gesture, and it deliberately does **not** touch the
//! timings — the full-cycle phasor keeps running underneath, so unpinning
//! lands wherever time says rather than resuming from the pinned entry.
//! [`GradientConfig::pinned_gradient`] is the one place the rule is read.
//!
//! Storage is the flattened recipe described in the [module docs](super) —
//! [`crate::LpValue`] has no union, so both variants share one struct: the
//! `kind` tag selects how `set` and the timings read (static ⇒ one-entry
//! set, zero timings). `set` is a variable-length list of [`Gradient`]
//! values, each carrying its stops as one compact literal.

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
        /// The one entry shown instead of the walk, when set — see the
        /// module docs' *Pinned*. Must index into `set`.
        pinned: Option<usize>,
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

    /// The entry a pinned cycle holds, when it is pinned.
    ///
    /// `None` for a static config (there is nothing to pin away from) and
    /// for an unpinned cycle. An out-of-range pin — which
    /// [`GradientConfig::validate`] rejects but a hand-built value can
    /// reach — reads as unpinned rather than panicking.
    #[must_use]
    pub fn pinned_gradient(&self) -> Option<&Gradient> {
        match self {
            Self::Cycle {
                set,
                pinned: Some(index),
                ..
            } => set.get(*index),
            _ => None,
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
            Self::Cycle { set, pinned, .. } => {
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
                if let Some(index) = *pinned
                    && index >= set.len()
                {
                    return Err(GradientError::PinnedOutOfRange(index));
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
// `kind` says how to read them: static ⇒ a one-entry `set`, timings `0.0`,
// no pin; cycle ⇒ 2..=8 entries. The set's length IS the count. `LpValue`
// has no option either, so `pinned` is an `i32` with [`NOT_PINNED`] for none.

/// Storage value of `pinned` when nothing is pinned.
const NOT_PINNED: i32 = -1;

impl ToLpValue for GradientConfig {
    fn to_lp_value(&self) -> LpValue {
        let (kind, gradients, step_seconds, fade_seconds, pinned) = match self {
            Self::Static(gradient) => (
                STATIC_KIND_TAG,
                core::slice::from_ref(gradient),
                0.0,
                0.0,
                NOT_PINNED,
            ),
            Self::Cycle {
                set,
                step_seconds,
                fade_seconds,
                pinned,
            } => (
                CYCLE_KIND_TAG,
                set.as_slice(),
                *step_seconds,
                *fade_seconds,
                pinned.map_or(NOT_PINNED, |index| index as i32),
            ),
        };

        let set: Vec<LpValue> = gradients.iter().map(ToLpValue::to_lp_value).collect();

        LpValue::Struct {
            name: Some("GradientConfig".to_string()),
            fields: Vec::from([
                ("kind".to_string(), kind.to_lp_value()),
                ("set".to_string(), LpValue::Array(set)),
                ("step_seconds".to_string(), step_seconds.to_lp_value()),
                ("fade_seconds".to_string(), fade_seconds.to_lp_value()),
                ("pinned".to_string(), pinned.to_lp_value()),
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
        let step_seconds: f32 = read_field(fields, 2, "GradientConfig", "step_seconds")?;
        let fade_seconds: f32 = read_field(fields, 3, "GradientConfig", "fade_seconds")?;
        let pinned: i32 = read_field(fields, 4, "GradientConfig", "pinned")?;

        match kind.as_str() {
            STATIC_KIND_TAG => {
                let mut set = read_gradient_set(fields, 1, 1)?;
                Ok(Self::Static(set.remove(0)))
            }
            CYCLE_KIND_TAG => Ok(Self::Cycle {
                set: read_gradient_set(fields, MIN_CYCLE_SET as usize, MAX_CYCLE_SET as usize)?,
                step_seconds,
                fade_seconds,
                // Any negative is "none"; an index past the set is kept so
                // `validate` can name it rather than silently dropping it.
                pinned: usize::try_from(pinned).ok(),
            }),
            other => Err(ValueRootError::new(alloc::format!(
                "unknown GradientConfig.kind {other:?}"
            ))),
        }
    }
}

/// Read the `set` list; its length must sit within the kind's bounds.
fn read_gradient_set(
    fields: &[(String, LpValue)],
    min: usize,
    max: usize,
) -> Result<Vec<Gradient>, ValueRootError> {
    let Some(("set", LpValue::Array(set))) =
        fields.get(1).map(|(name, value)| (name.as_str(), value))
    else {
        return Err(ValueRootError::new("expected GradientConfig.set"));
    };
    if !(min..=max).contains(&set.len()) {
        return Err(ValueRootError::new(alloc::format!(
            "GradientConfig.set must hold {min}..={max} entries for this kind, got {}",
            set.len()
        )));
    }
    set.iter().map(Gradient::from_lp_value).collect()
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
            ty: StaticLpType::List(&GRADIENT_STATIC_TYPE),
        },
        StaticModelStructMember {
            name: "step_seconds",
            ty: StaticLpType::F32,
        },
        StaticModelStructMember {
            name: "fade_seconds",
            ty: StaticLpType::F32,
        },
        StaticModelStructMember {
            name: "pinned",
            ty: StaticLpType::I32,
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
                ty: LpType::List(alloc::boxed::Box::new(gradient_lp_type())),
            },
            ModelStructMember {
                name: "step_seconds".to_string(),
                ty: LpType::F32,
            },
            ModelStructMember {
                name: "fade_seconds".to_string(),
                ty: LpType::F32,
            },
            ModelStructMember {
                name: "pinned".to_string(),
                ty: LpType::I32,
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
            pinned: None,
        }
    }

    fn pinned(count: usize, index: usize) -> GradientConfig {
        let GradientConfig::Cycle {
            set,
            step_seconds,
            fade_seconds,
            ..
        } = cycle(count, 2.0)
        else {
            unreachable!()
        };
        GradientConfig::Cycle {
            set,
            step_seconds,
            fade_seconds,
            pinned: Some(index),
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
                pinned: None,
            }
            .validate(),
            Err(GradientError::CycleEntry(1))
        );
    }

    #[test]
    fn a_pin_names_one_entry_and_leaves_the_timings_alone() {
        let config = pinned(4, 2);

        assert_eq!(config.validate(), Ok(()));
        assert_eq!(config.pinned_gradient(), Some(&swatches(4)[2]));
        // The phasor keeps running underneath so unpinning lands where time
        // says — a pin is not a freeze.
        assert!(!config.is_frozen());
        assert_eq!(config.full_cycle_seconds(), 8.0);

        assert_eq!(cycle(4, 2.0).pinned_gradient(), None);
        assert_eq!(GradientConfig::default().pinned_gradient(), None);
    }

    #[test]
    fn validate_rejects_a_pin_past_the_set() {
        assert_eq!(
            pinned(3, 3).validate(),
            Err(GradientError::PinnedOutOfRange(3))
        );
        assert_eq!(pinned(3, 3).pinned_gradient(), None, "reads as unpinned");
    }

    #[test]
    fn storage_writes_minus_one_for_no_pin_and_the_index_for_a_pin() {
        let pin_field = |config: &GradientConfig| {
            let LpValue::Struct { fields, .. } = config.to_lp_value() else {
                panic!("GradientConfig storage must be a Struct");
            };
            fields[4].clone()
        };

        assert_eq!(
            pin_field(&GradientConfig::default()),
            ("pinned".to_string(), LpValue::I32(-1))
        );
        assert_eq!(
            pin_field(&cycle(3, 1.0)),
            ("pinned".to_string(), LpValue::I32(-1))
        );
        assert_eq!(
            pin_field(&pinned(3, 1)),
            ("pinned".to_string(), LpValue::I32(1))
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
            pinned(3, 0),
            pinned(MAX_CYCLE_SET as usize, 7),
        ] {
            assert_eq!(
                GradientConfig::from_lp_value(&config.to_lp_value()).unwrap(),
                config
            );
        }
    }

    /// Static writes zero timings and a ONE-entry set — the set's length
    /// IS the count, there is no separate field.
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
        assert_eq!(fields[2], ("step_seconds".to_string(), LpValue::F32(0.0)));
        assert_eq!(fields[3], ("fade_seconds".to_string(), LpValue::F32(0.0)));
        assert_eq!(fields[4], ("pinned".to_string(), LpValue::I32(-1)));

        let LpValue::Array(set) = &fields[1].1 else {
            panic!("set must be an Array");
        };
        assert_eq!(set.len(), 1);
    }

    /// The set length must sit within the KIND's bounds: exactly one for
    /// static, 2..=8 for a cycle.
    #[test]
    fn storage_rejects_a_set_length_the_kind_tag_disallows() {
        let with_kind_and_set = |kind: &str, entries: usize| {
            let LpValue::Struct { name, mut fields } = cycle(2, 1.0).to_lp_value() else {
                unreachable!()
            };
            fields[0].1 = LpValue::String(kind.to_string());
            fields[1].1 = LpValue::Array(
                (0..entries)
                    .map(|_| Gradient::default().to_lp_value())
                    .collect(),
            );
            LpValue::Struct { name, fields }
        };

        assert!(GradientConfig::from_lp_value(&with_kind_and_set("static", 2)).is_err());
        assert!(GradientConfig::from_lp_value(&with_kind_and_set("cycle", 1)).is_err());
        assert!(GradientConfig::from_lp_value(&with_kind_and_set("cycle", 9)).is_err());
        assert!(GradientConfig::from_lp_value(&with_kind_and_set("playlist", 2)).is_err());
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
