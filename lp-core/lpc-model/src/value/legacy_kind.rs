//! Legacy quantity [`Kind`] model.
//!
//! This module is retained for older source/runtime code that still describes
//! authored properties as a semantic kind plus independent storage. New slot
//! model work should prefer typed slot leaf descriptors where semantic meaning
//! owns its storage shape instead of being attached to an arbitrary value.
//!
//! [`Kind`]: semantic identity of a value.
//!
//! In the five-layer Quantity model, [`Kind`] sits *above* storage types and
//! *below* per-slot legality: it answers “what category of thing is this value?”
//! while [`crate::LpType`] and runtime shader value types cover raw
//! shape. See `docs/design/lightplayer/quantity.md` §0, §1, §2, and §3.
//!
//! A [`Kind`] is **orthogonal to storage** in the sense that each variant maps
//! to a **fixed** [`Kind::storage`] recipe for GPU and serialization, while
//! still carrying a distinct *meaning* (e.g. [`Kind::Amplitude`] vs
//! [`Kind::Ratio`] both use `LpType::F32` in v0 but differ in default
//! constraint, presentation, and intent — see the §3 table in `quantity.md`).

use crate::value::constraint::{Constraint, ConstraintFree, ConstraintRange};
use crate::value::lp_type::{LpType, ModelStructMember};
use alloc::string::String;

/// The color-family vocabulary now lives in [`crate::color`], which owns both
/// the authored serde surface and the `color.md` §5 storage recipe. Re-exported
/// here so the historical `kind::` paths keep resolving.
pub use crate::color::{Colorspace, InterpMethod, MAX_GRADIENT_STOPS};

/// Number of frequency bands carried by [`Kind::AudioLevel`]: low / mid /
/// high. See `docs/design/lightplayer/quantity.md` §3.
pub const AUDIO_LEVEL_BANDS: usize = 3;

/// **Commensurability class** for a [`Kind`]: two Kinds share a [`Dimension`]
/// iff their values are meaningfully expressed in the same *kind* of unit.
///
/// This is the “which quantities can be converted as same-dimension” layer;
/// the framework does **not** do quantity algebra — math stays in user shaders
/// (`docs/design/lightplayer/quantity.md` §4, “No quantity arithmetic in the
/// framework”). [`Kind::Phase`] and [`Kind::Angle`] are intentionally *different*
/// [`Kind`]s even though both are “dimensionless” in the SI sense (`quantity.md` §4).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub enum Dimension {
    /// Kinds with no physical dimension: [`Kind::Amplitude`], [`Kind::Ratio`], [`Kind::Phase`], [`Kind::Count`], [`Kind::Bool`], [`Kind::Choice`], color-family and spatial-struct Kinds, and [`Kind::Texture`].
    Dimensionless,
    /// [`Kind::Instant`] and [`Kind::Duration`] (stored in seconds as F32, `quantity.md` §4).
    Time,
    /// [`Kind::Frequency`] (stored in hertz, `quantity.md` §4).
    Frequency,
    /// [`Kind::Angle`] (stored in radians, `quantity.md` §4).
    Angle,
}

/// **Base storage unit** implied by a [`Kind`] for display and v0 TOML (no
/// per-value `unit` field — the [`Kind`] implies it, `quantity.md` §4).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub enum Unit {
    /// Used for [`Dimension::Dimensionless`] (and in v0, [`Kind::Phase`], which is *not* [`Dimension::Angle`], `quantity.md` §4).
    None,
    /// Time base unit (seconds) for the [`Dimension::Time`] dimension.
    Seconds,
    /// Frequency base unit (hertz) for the [`Dimension::Frequency`] dimension.
    Hertz,
    /// Angle base unit (radians) for the [`Dimension::Angle`] dimension.
    Radians,
}

/// **Semantic** identity of a value in the LightPlayer domain: what it *means*
/// for tooling, the bus, and defaults — independent of whether it is a scalar
/// or a structured *value-type* vs an opaque *handle-type* (`quantity.md` §3, open
/// enumeration).
///
/// The set is open for new examples; M2 ships the v0 row used across documentation.
///
/// Grouping (per `quantity.md` §3 sketch):
///
/// - **Dimensionless value scalars:** `Kind::Amplitude`, `Kind::Ratio`, `Kind::Phase`, `Kind::Count`, `Kind::Bool`, `Kind::Choice`
/// - **Scalars with a [`Dimension`]:** `Kind::Instant`, `Kind::Duration`, `Kind::Frequency`, `Kind::Angle`
/// - **Structured *value* kinds (GPU-friendly structs):** `Kind::Color`, `Kind::Gradient`, `Kind::Position2d`, `Kind::Position3d`, `Kind::AudioLevel`
/// - **Opaque handle (texture today):** `Kind::Texture`
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// [0, 1] strength of a signal (`quantity.md` §3 table).
    Amplitude,
    /// [0, 1] fraction or proportion (`quantity.md` §3).
    Ratio,
    /// [0, 1) **wrapping** cycle position; distinct from [`Kind::Angle`] and [`Kind::Ratio`] by intent (`quantity.md` §3; see also `docs/roadmaps/2026-04-22-lp-domain/notes-quantity.md` Q3 for `Phase` vs `Angle`).
    Phase,
    /// Non-negative integer count; storage `LpType::I32` (`quantity.md` §3, storage table).
    Count,
    /// Boolean.
    Bool,
    /// Discrete choice; storage `LpType::I32` in v0 (`quantity.md` §3).
    Choice,

    /// Time **instant** as F32 seconds since an epoch; [`Dimension::Time`] (`quantity.md` §3). Default bus is [`Binding::Bus`] with channel `"time"` when no explicit bind (`quantity.md` §8).
    Instant,
    /// Non-negative F32 **duration** in seconds; [`Dimension::Time`] (`quantity.md` §3).
    Duration,
    /// F32 **frequency** in hertz; [`Dimension::Frequency`] (`quantity.md` §3).
    Frequency,
    /// F32 **angle** in radians, may exceed 2π; [`Dimension::Angle`] (`quantity.md` §3).
    Angle,

    /// Full color in an author-selected space; see `docs/design/color.md` and the struct recipe in `quantity.md` §3.
    Color,
    /// Palette: stops in one space, read one way — [`MAX_GRADIENT_STOPS`] and
    /// [`InterpMethod`], defined by [`crate::Gradient`].
    ///
    /// The semantic tag for the whole color-collection family: a discrete
    /// swatch list is a gradient with [`InterpMethod::Step`], which is why
    /// there is no separate `ColorPalette` kind.
    ///
    /// Note: This is the **authoring/storage** recipe. At runtime the engine bakes the
    /// gradient to a height-one texture and binds it as a shader field like `params.gradient`.
    Gradient,
    /// 2D position as `LpType::Vec2` (`quantity.md` §3).
    Position2d,
    /// 3D position as `LpType::Vec3` (`quantity.md` §3).
    Position3d,

    /// Opaque **texture** semantic kind: portable struct storage (`width` /
    /// `height` / `handle` / …) describes serialization and GPU-oriented
    /// layout intent, not the same thing as a lazy visual product value.
    Texture,

    /// Audio frequency-band levels (low / mid / high) as F32 RMS values.
    /// Default-binds to `audio/in/0/level` (`quantity.md` §8). Storage is a
    /// fixed `{ low: f32, mid: f32, high: f32 }` struct (no project-wide
    /// constant beyond [`AUDIO_LEVEL_BANDS`]). Default constraint is
    /// [`Constraint::Free`] — RMS may exceed 1.0 with boost; clamping is
    /// downstream policy. See `docs/design/lightplayer/quantity.md` §3.
    AudioLevel,
}

impl Kind {
    /// Returns the **structural** [`LpType`] the serializer and layout logic
    /// agree on: the “storage recipe” for this [`Kind`]
    /// (`docs/design/lightplayer/quantity.md` §3, “Storage recipes”, and `impl`
    /// block in §3).
    ///
    /// For `Gradient`, this is the **authoring** storage type. The
    /// shader-visible runtime form is a baked texture field inside `params`.
    pub fn storage(self) -> LpType {
        match self {
            Self::Amplitude
            | Self::Ratio
            | Self::Phase
            | Self::Instant
            | Self::Duration
            | Self::Frequency
            | Self::Angle => LpType::F32,
            Self::Count | Self::Choice => LpType::I32,
            Self::Bool => LpType::Bool,
            Self::Position2d => LpType::Vec2,
            Self::Position3d => LpType::Vec3,
            Self::Color => color_struct(),
            Self::Gradient => crate::color::gradient_lp_type(),
            Self::Texture => texture_struct(),
            Self::AudioLevel => audio_level_struct(),
        }
    }

    /// Returns the **dimensional** class used for commensurability
    /// (`docs/design/lightplayer/quantity.md` §4, [`Dimension`] / [`Unit`]). Kinds
    /// not listed in that section map to [`Dimension::Dimensionless`], including
    /// [`Kind::Phase`] (explicitly *not* [`Dimension::Angle`], `quantity.md` §4).
    pub fn dimension(self) -> Dimension {
        match self {
            Self::Instant | Self::Duration => Dimension::Time,
            Self::Frequency => Dimension::Frequency,
            Self::Angle => Dimension::Angle,
            _ => Dimension::Dimensionless,
        }
    }

    /// A **sensible default** [`Constraint::Range`] (or [`Constraint::Free`])
    /// for this [`Kind`] when a slot does not override legality. This is the
    /// *natural* domain of the kind before slot-specific tuning (`quantity.md` §3
    /// per-`Kind` contract and §5: constraints **refine** the kind, they don’t
    /// replace it).
    ///
    /// v0 range fields are F32 in [`crate::value::constraint::Constraint`]; see
    /// `docs/plans-old/2026-04-22-lp-domain-m2-domain-skeleton/summary.md` (F32
    /// narrowing and future widening).
    pub fn default_constraint(self) -> Constraint {
        match self {
            Self::Amplitude | Self::Ratio | Self::Phase => Constraint::Range(ConstraintRange {
                range: [0.0, 1.0],
                step: None,
            }),
            Self::Count => Constraint::Range(ConstraintRange {
                range: [0.0, 2_147_483_647.0],
                step: None,
            }),
            _ => Constraint::Free(ConstraintFree {}),
        }
    }
}

fn color_struct() -> LpType {
    LpType::Struct {
        name: Some(String::from("Color")),
        fields: alloc::vec![
            ModelStructMember {
                name: String::from("space"),
                ty: LpType::I32,
            },
            ModelStructMember {
                name: String::from("coords"),
                ty: LpType::Vec3,
            },
        ],
    }
}

fn texture_struct() -> LpType {
    LpType::Struct {
        name: Some(String::from("Texture")),
        fields: alloc::vec![
            ModelStructMember {
                name: String::from("format"),
                ty: LpType::I32,
            },
            ModelStructMember {
                name: String::from("width"),
                ty: LpType::I32,
            },
            ModelStructMember {
                name: String::from("height"),
                ty: LpType::I32,
            },
            ModelStructMember {
                name: String::from("handle"),
                ty: LpType::I32,
            },
        ],
    }
}

fn audio_level_struct() -> LpType {
    LpType::Struct {
        name: Some(String::from("AudioLevel")),
        fields: alloc::vec![
            ModelStructMember {
                name: String::from("low"),
                ty: LpType::F32,
            },
            ModelStructMember {
                name: String::from("mid"),
                ty: LpType::F32,
            },
            ModelStructMember {
                name: String::from("high"),
                ty: LpType::F32,
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_is_exhaustive_and_concrete() {
        for k in [
            Kind::Amplitude,
            Kind::Ratio,
            Kind::Phase,
            Kind::Count,
            Kind::Bool,
            Kind::Choice,
            Kind::Instant,
            Kind::Duration,
            Kind::Frequency,
            Kind::Angle,
            Kind::Color,
            Kind::Gradient,
            Kind::Position2d,
            Kind::Position3d,
            Kind::Texture,
            Kind::AudioLevel,
        ] {
            let _ = k.storage();
        }
    }

    #[test]
    fn float_scalar_storages() {
        assert_eq!(Kind::Amplitude.storage(), LpType::F32);
        assert_eq!(Kind::Frequency.storage(), LpType::F32);
    }

    #[test]
    fn int_scalar_storages() {
        assert_eq!(Kind::Count.storage(), LpType::I32);
        assert_eq!(Kind::Choice.storage(), LpType::I32);
    }

    #[test]
    fn position_storages() {
        assert_eq!(Kind::Position2d.storage(), LpType::Vec2);
        assert_eq!(Kind::Position3d.storage(), LpType::Vec3);
    }

    #[test]
    fn texture_storage_has_four_int_fields() {
        let s = Kind::Texture.storage();
        match s {
            LpType::Struct { fields, .. } => {
                assert_eq!(fields.len(), 4);
                for m in fields {
                    assert_eq!(m.ty, LpType::I32);
                }
            }
            _ => panic!("Texture storage must be a Struct"),
        }
    }

    #[test]
    fn dimension_assignment() {
        assert_eq!(Kind::Instant.dimension(), Dimension::Time);
        assert_eq!(Kind::Duration.dimension(), Dimension::Time);
        assert_eq!(Kind::Frequency.dimension(), Dimension::Frequency);
        assert_eq!(Kind::Angle.dimension(), Dimension::Angle);
        assert_eq!(Kind::Amplitude.dimension(), Dimension::Dimensionless);
        assert_eq!(Kind::Phase.dimension(), Dimension::Dimensionless);
    }

    #[test]
    fn default_constraint_for_amplitude_is_unit_range() {
        match Kind::Amplitude.default_constraint() {
            Constraint::Range(ConstraintRange { range, step }) => {
                assert_eq!(range, [0.0, 1.0]);
                assert!(step.is_none());
            }
            _ => panic!("expected Range[0,1]"),
        }
    }

    #[test]
    fn kind_serde_round_trips() {
        let k = Kind::Frequency;
        let s = serde_json::to_string(&k).unwrap();
        assert_eq!(s, "\"frequency\"");
        let back: Kind = serde_json::from_str(&s).unwrap();
        assert_eq!(k, back);
    }

    #[test]
    fn audio_level_storage_is_three_floats() {
        let s = Kind::AudioLevel.storage();
        match s {
            LpType::Struct { fields, .. } => {
                assert_eq!(fields.len(), AUDIO_LEVEL_BANDS);
                assert_eq!(fields[0].name.as_str(), "low");
                assert_eq!(fields[1].name.as_str(), "mid");
                assert_eq!(fields[2].name.as_str(), "high");
                for m in &fields {
                    assert_eq!(m.ty, LpType::F32);
                }
            }
            _ => panic!("AudioLevel storage must be a Struct"),
        }
    }

    #[test]
    fn audio_level_dimension_is_dimensionless() {
        assert_eq!(Kind::AudioLevel.dimension(), Dimension::Dimensionless);
    }

    #[test]
    fn audio_level_default_constraint_is_free() {
        assert!(matches!(
            Kind::AudioLevel.default_constraint(),
            Constraint::Free(ConstraintFree {})
        ));
    }

    #[test]
    fn audio_level_serializes_as_snake_case() {
        let k = Kind::AudioLevel;
        let s = serde_json::to_string(&k).unwrap();
        assert_eq!(s, "\"audio_level\"");
    }
}
