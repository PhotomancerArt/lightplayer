//! The [`PlaylistTour`] value: how a playlist walks its entries over time.
//!
//! It is the palette's `Static | Cycle` ([`crate::GradientConfig`]) for
//! patterns (multi-pattern vision D13), and it follows the same rules:
//!
//! - **Config, never state.** Where a cycling playlist is is a pure function
//!   of the playlist's consumed `time` and an anchor the engine keeps; the
//!   value only says how fast to walk and how long each hand-off fades.
//! - **`step_seconds <= 0` (or non-finite) is frozen**, the same rule and
//!   the same words as [`crate::GradientConfig::is_frozen`]:
//!   [`PlaylistTour::is_frozen`] is the one place it lives here. A frozen
//!   tour behaves exactly like [`PlaylistTour::Hold`] — the playlist's
//!   idle entry, triggers and per-entry durations, unchanged (vision D17).
//!
//! Storage is a flattened record, because [`LpValue`] has no union: both
//! variants write the same three fields and `kind` says how to read them
//! (hold ⇒ zero timings).
//!
//! ```json
//! "tour": { "kind": "cycle", "step_seconds": 20, "fade_seconds": 1.5 }
//! ```

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::color::gradient::read_field;
use crate::{
    FromLpValue, LpType, LpValue, ModelStructMember, SlotMeta, SlotShape, SlotShapeId, SlotValue,
    SlotValueShape, StaticLpType, StaticModelStructMember, StaticSlotMeta, StaticSlotShape,
    StaticSlotShapeDescriptor, StaticSlotValueShape, StaticValueEditorHint, ToLpValue,
    ValueEditorHint, ValueRootError,
};

/// Native shape name for [`PlaylistTour`].
pub const PLAYLIST_TOUR_SHAPE_NAME: &str = "lp::playlist::PlaylistTour";

/// The record name [`PlaylistTour`] storage carries.
const STRUCT_NAME: &str = "PlaylistTour";

/// Wire tag for [`PlaylistTour::Hold`] in [`LpValue`] storage.
const HOLD_KIND_TAG: &str = "hold";

/// Wire tag for [`PlaylistTour::Cycle`] in [`LpValue`] storage.
const CYCLE_KIND_TAG: &str = "cycle";

/// How a playlist walks its entries over time — the palette's
/// Static|Cycle, for patterns (vision D13).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum PlaylistTour {
    /// Stay where the playlist is: the idle entry, triggers and per-entry
    /// durations, exactly as a playlist without a tour behaves.
    #[default]
    Hold,
    /// Walk every enabled entry in key order, one step each, wrapping.
    Cycle {
        /// Seconds each entry plays, on the playlist's clock. `<= 0` or
        /// non-finite is frozen.
        step_seconds: f32,
        /// The fade at each hand-off, in seconds.
        fade_seconds: f32,
    },
}

impl PlaylistTour {
    /// Whether this tour holds still.
    ///
    /// [`PlaylistTour::Hold`] always does; a cycle does when its step is
    /// non-positive or non-finite. The one place the frozen rule is decided
    /// for playlists — everything downstream asks here.
    #[must_use]
    pub fn is_frozen(&self) -> bool {
        match self {
            Self::Hold => true,
            Self::Cycle { step_seconds, .. } => !step_seconds.is_finite() || *step_seconds <= 0.0,
        }
    }

    /// The step of a running cycle, `None` when frozen.
    #[must_use]
    pub fn running_step_seconds(&self) -> Option<f32> {
        match self {
            Self::Cycle { step_seconds, .. } if !self.is_frozen() => Some(*step_seconds),
            _ => None,
        }
    }

    /// The fade at each hand-off; `0.0` for a hold.
    #[must_use]
    pub fn fade_seconds(&self) -> f32 {
        match self {
            Self::Hold => 0.0,
            Self::Cycle { fade_seconds, .. } => *fade_seconds,
        }
    }
}

impl ToLpValue for PlaylistTour {
    fn to_lp_value(&self) -> LpValue {
        let (kind, step_seconds, fade_seconds) = match self {
            Self::Hold => (HOLD_KIND_TAG, 0.0, 0.0),
            Self::Cycle {
                step_seconds,
                fade_seconds,
            } => (CYCLE_KIND_TAG, *step_seconds, *fade_seconds),
        };
        LpValue::Struct {
            name: Some(STRUCT_NAME.to_string()),
            fields: Vec::from([
                ("kind".to_string(), kind.to_lp_value()),
                ("step_seconds".to_string(), step_seconds.to_lp_value()),
                ("fade_seconds".to_string(), fade_seconds.to_lp_value()),
            ]),
        }
    }
}

impl FromLpValue for PlaylistTour {
    fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
        let LpValue::Struct { name, fields } = value else {
            return Err(ValueRootError::new("expected PlaylistTour struct"));
        };
        if name.as_deref() != Some(STRUCT_NAME) || fields.len() != 3 {
            return Err(ValueRootError::new("expected PlaylistTour struct"));
        }
        let kind: String = read_field(fields, 0, STRUCT_NAME, "kind")?;
        let step_seconds: f32 = read_field(fields, 1, STRUCT_NAME, "step_seconds")?;
        let fade_seconds: f32 = read_field(fields, 2, STRUCT_NAME, "fade_seconds")?;
        match kind.as_str() {
            HOLD_KIND_TAG => Ok(Self::Hold),
            CYCLE_KIND_TAG => Ok(Self::Cycle {
                step_seconds,
                fade_seconds,
            }),
            other => Err(ValueRootError::new(alloc::format!(
                "unknown PlaylistTour.kind {other:?}"
            ))),
        }
    }
}

const PLAYLIST_TOUR_STATIC_TYPE: StaticLpType = StaticLpType::Struct {
    name: Some(STRUCT_NAME),
    fields: &[
        StaticModelStructMember {
            name: "kind",
            ty: StaticLpType::String,
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

const PLAYLIST_TOUR_STATIC_META: StaticSlotMeta = StaticSlotMeta {
    label: Some("Tour"),
    description: Some(
        "Hold, or cycle through the enabled entries: seconds per entry and the fade between them.",
    ),
    unit: None,
};

/// The canonical [`PlaylistTour`] storage recipe.
#[must_use]
pub fn playlist_tour_lp_type() -> LpType {
    LpType::Struct {
        name: Some(STRUCT_NAME.to_string()),
        fields: Vec::from([
            ModelStructMember {
                name: "kind".to_string(),
                ty: LpType::String,
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

impl SlotValue for PlaylistTour {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name(PLAYLIST_TOUR_SHAPE_NAME);
    const STATIC_VALUE_SHAPE_DESCRIPTOR: Option<StaticSlotValueShape> =
        Some(StaticSlotValueShape {
            id: <PlaylistTour as SlotValue>::SHAPE_ID,
            ty: PLAYLIST_TOUR_STATIC_TYPE,
            meta: PLAYLIST_TOUR_STATIC_META,
            editor: StaticValueEditorHint::Plain,
        });

    fn value_shape() -> SlotValueShape {
        SlotValueShape {
            id: <PlaylistTour as SlotValue>::SHAPE_ID,
            ty: playlist_tour_lp_type(),
            meta: SlotMeta {
                label: PLAYLIST_TOUR_STATIC_META.label.map(ToString::to_string),
                description: PLAYLIST_TOUR_STATIC_META
                    .description
                    .map(ToString::to_string),
                unit: None,
            },
            editor: ValueEditorHint::Plain,
        }
    }
}

impl StaticSlotShape for PlaylistTour {
    const SHAPE_ID: SlotShapeId = <Self as SlotValue>::SHAPE_ID;
    const STATIC_SLOT_SHAPE_DESCRIPTOR: Option<&'static StaticSlotShapeDescriptor> =
        Some(&StaticSlotShapeDescriptor::Value {
            shape: StaticSlotValueShape {
                id: <PlaylistTour as SlotValue>::SHAPE_ID,
                ty: PLAYLIST_TOUR_STATIC_TYPE,
                meta: PLAYLIST_TOUR_STATIC_META,
                editor: StaticValueEditorHint::Plain,
            },
        });

    fn slot_shape() -> SlotShape {
        SlotShape::leaf(<Self as SlotValue>::value_shape())
    }

    fn shape_name() -> Option<&'static str> {
        Some(PLAYLIST_TOUR_SHAPE_NAME)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cycle(step_seconds: f32) -> PlaylistTour {
        PlaylistTour::Cycle {
            step_seconds,
            fade_seconds: 0.5,
        }
    }

    #[test]
    fn the_default_is_hold_and_hold_is_frozen() {
        assert_eq!(PlaylistTour::default(), PlaylistTour::Hold);
        assert!(PlaylistTour::Hold.is_frozen());
        assert_eq!(PlaylistTour::Hold.running_step_seconds(), None);
        assert_eq!(PlaylistTour::Hold.fade_seconds(), 0.0);
    }

    #[test]
    fn a_positive_step_runs() {
        assert!(!cycle(2.0).is_frozen());
        assert_eq!(cycle(2.0).running_step_seconds(), Some(2.0));
        assert_eq!(cycle(2.0).fade_seconds(), 0.5);
    }

    /// The same rule as `GradientConfig::is_frozen`.
    #[test]
    fn non_positive_and_non_finite_steps_are_frozen() {
        for step in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert!(cycle(step).is_frozen(), "step {step} should freeze");
            assert_eq!(cycle(step).running_step_seconds(), None);
        }
    }

    #[test]
    fn both_variants_round_trip_through_lp_value() {
        for tour in [PlaylistTour::Hold, cycle(2.0), cycle(0.0)] {
            assert_eq!(
                PlaylistTour::from_lp_value(&tour.to_lp_value()).unwrap(),
                tour
            );
        }
    }

    #[test]
    fn storage_is_a_flattened_three_field_record() {
        let LpValue::Struct { name, fields } = cycle(8.0).to_lp_value() else {
            panic!("PlaylistTour storage must be a Struct");
        };
        assert_eq!(name.as_deref(), Some("PlaylistTour"));
        assert_eq!(
            fields,
            Vec::from([
                ("kind".to_string(), LpValue::String("cycle".to_string())),
                ("step_seconds".to_string(), LpValue::F32(8.0)),
                ("fade_seconds".to_string(), LpValue::F32(0.5)),
            ])
        );
    }

    #[test]
    fn an_unknown_kind_is_rejected() {
        let LpValue::Struct { name, mut fields } = cycle(1.0).to_lp_value() else {
            unreachable!()
        };
        fields[0].1 = LpValue::String("shuffle".to_string());
        let error = PlaylistTour::from_lp_value(&LpValue::Struct { name, fields }).unwrap_err();
        assert!(error.message.contains("shuffle"), "{}", error.message);
    }

    #[test]
    fn static_and_dynamic_playlist_tour_shapes_agree() {
        let dynamic = <PlaylistTour as SlotValue>::value_shape();
        let static_shape = <PlaylistTour as SlotValue>::STATIC_VALUE_SHAPE_DESCRIPTOR
            .expect("static descriptor");

        assert_eq!(static_shape.to_owned_value_shape(), dynamic);
        assert_eq!(
            dynamic.id,
            SlotShapeId::from_static_name(PLAYLIST_TOUR_SHAPE_NAME)
        );
    }
}
