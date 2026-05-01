//! [`Kind`]: semantic identity of a value.
//!
//! In the five-layer Quantity model, [`Kind`] sits *above* storage types and
//! *below* per-slot legality: it answers “what category of thing is this value?”
//! while [`crate::ModelType`] and runtime shader value types cover raw
//! shape. See `docs/design/lightplayer/quantity.md` §0, §1, §2, and §3.
//!
//! A [`Kind`] is **orthogonal to storage** in the sense that each variant maps
//! to a **fixed** [`Kind::storage`] recipe for GPU and serialization, while
//! still carrying a distinct *meaning* (e.g. [`Kind::Amplitude`] vs
//! [`Kind::Ratio`] both use `ModelType::F32` in v0 but differ in default
//! constraint, presentation, and intent — see the §3 table in `quantity.md`).

use crate::prop::constraint::{Constraint, ConstraintFree, ConstraintRange};
use crate::prop::model_type::{ModelStructMember, ModelType};
use alloc::boxed::Box;
use alloc::string::String;

/// Maximum number of colors in a [`Kind::ColorPalette`] value’s fixed array storage.
///
/// v0 is deliberately small for embedded targets; the same constant sizes the
/// `entries` field in the palette’s [`ModelType`] (see `quantity.md` §3 “Storage
/// recipes” and the roadmap risk note on fixed-size arrays).
pub const MAX_PALETTE_LEN: u32 = 16;

/// Maximum number of stops in a [`Kind::Gradient`] value’s fixed `stops` array.
///
/// See [`MAX_PALETTE_LEN`] and `quantity.md` §3. Constants like this live in
/// `lp-domain` so layout stays explicit next to the [`Kind`]s that use them.
pub const MAX_GRADIENT_STOPS: u32 = 16;

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

/// Authoritative color space tag used **inside** color-family runtime structs; values line up with `docs/design/color.md` and the `space: I32` field in the [`Kind::Color`]/[`Kind::ColorPalette`]/[`Kind::Gradient`] storage recipes (`quantity.md` §3).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub enum Colorspace {
    Oklch,
    Oklab,
    LinearRgb,
    Srgb,
}

/// How to interpolate a [`Kind::Gradient`]; the numeric tag lives in the gradient struct’s `method: I32` field (`quantity.md` §3, `color.md`).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub enum InterpMethod {
    Linear,
    Cubic,
    Step,
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
/// - **Structured *value* kinds (GPU-friendly structs):** `Kind::Color`, `Kind::ColorPalette`, `Kind::Gradient`, `Kind::Position2d`, `Kind::Position3d`, `Kind::AudioLevel`
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
    /// Non-negative integer count; storage `ModelType::I32` (`quantity.md` §3, storage table).
    Count,
    /// Boolean.
    Bool,
    /// Discrete choice; storage `ModelType::I32` in v0 (`quantity.md` §3).
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
    /// Fixed-max palette: [`MAX_PALETTE_LEN`], `count`, and `entries` (`quantity.md` §3, `color.md`).
    ///
    /// Note: This is the **authoring/storage** recipe. At runtime, lpfx bakes the palette
    /// to a height-one texture and binds it as a shader field like `params.palette`.
    ColorPalette,
    /// Gradient with stops; [`MAX_GRADIENT_STOPS`] and [`InterpMethod`] (`quantity.md` §3).
    ///
    /// Note: This is the **authoring/storage** recipe. At runtime, lpfx bakes the gradient
    /// to a height-one texture and binds it as a shader field like `params.gradient`.
    Gradient,
    /// 2D position as `ModelType::Vec2` (`quantity.md` §3).
    Position2d,
    /// 3D position as `ModelType::Vec3` (`quantity.md` §3).
    Position3d,

    /// Opaque **texture** semantic kind: portable struct storage (`width` /
    /// `height` / `handle` / …) describes serialization and GPU-oriented
    /// layout intent—not the same thing as a full-frame
    /// engine-managed visual product (`RuntimeProduct::Render` in `lpc-engine`).
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
    /// Returns the **structural** [`ModelType`] the serializer and layout logic
    /// agree on: the “storage recipe” for this [`Kind`]
    /// (`docs/design/lightplayer/quantity.md` §3, “Storage recipes”, and `impl`
    /// block in §3).
    ///
    /// For `ColorPalette` and `Gradient`, this is the **authoring** storage type.
    /// The shader-visible runtime form is a baked texture field inside `params`.
    pub fn storage(self) -> ModelType {
        match self {
            Self::Amplitude
            | Self::Ratio
            | Self::Phase
            | Self::Instant
            | Self::Duration
            | Self::Frequency
            | Self::Angle => ModelType::F32,
            Self::Count | Self::Choice => ModelType::I32,
            Self::Bool => ModelType::Bool,
            Self::Position2d => ModelType::Vec2,
            Self::Position3d => ModelType::Vec3,
            Self::Color => color_struct(),
            Self::ColorPalette => color_palette_struct(),
            Self::Gradient => gradient_struct(),
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
    /// v0 range fields are F32 in [`crate::prop::constraint::Constraint`]; see
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

fn color_struct() -> ModelType {
    ModelType::Struct {
        name: Some(String::from("Color")),
        fields: alloc::vec![
            ModelStructMember {
                name: String::from("space"),
                ty: ModelType::I32,
            },
            ModelStructMember {
                name: String::from("coords"),
                ty: ModelType::Vec3,
            },
        ],
    }
}

fn color_palette_struct() -> ModelType {
    ModelType::Struct {
        name: Some(String::from("ColorPalette")),
        fields: alloc::vec![
            ModelStructMember {
                name: String::from("space"),
                ty: ModelType::I32,
            },
            ModelStructMember {
                name: String::from("count"),
                ty: ModelType::I32,
            },
            ModelStructMember {
                name: String::from("entries"),
                ty: ModelType::Array(Box::new(ModelType::Vec3), MAX_PALETTE_LEN as usize),
            },
        ],
    }
}

fn gradient_struct() -> ModelType {
    let stop = ModelType::Struct {
        name: Some(String::from("GradientStop")),
        fields: alloc::vec![
            ModelStructMember {
                name: String::from("at"),
                ty: ModelType::F32,
            },
            ModelStructMember {
                name: String::from("c"),
                ty: ModelType::Vec3,
            },
        ],
    };
    ModelType::Struct {
        name: Some(String::from("Gradient")),
        fields: alloc::vec![
            ModelStructMember {
                name: String::from("space"),
                ty: ModelType::I32,
            },
            ModelStructMember {
                name: String::from("method"),
                ty: ModelType::I32,
            },
            ModelStructMember {
                name: String::from("count"),
                ty: ModelType::I32,
            },
            ModelStructMember {
                name: String::from("stops"),
                ty: ModelType::Array(Box::new(stop), MAX_GRADIENT_STOPS as usize),
            },
        ],
    }
}

fn texture_struct() -> ModelType {
    ModelType::Struct {
        name: Some(String::from("Texture")),
        fields: alloc::vec![
            ModelStructMember {
                name: String::from("format"),
                ty: ModelType::I32,
            },
            ModelStructMember {
                name: String::from("width"),
                ty: ModelType::I32,
            },
            ModelStructMember {
                name: String::from("height"),
                ty: ModelType::I32,
            },
            ModelStructMember {
                name: String::from("handle"),
                ty: ModelType::I32,
            },
        ],
    }
}

fn audio_level_struct() -> ModelType {
    ModelType::Struct {
        name: Some(String::from("AudioLevel")),
        fields: alloc::vec![
            ModelStructMember {
                name: String::from("low"),
                ty: ModelType::F32,
            },
            ModelStructMember {
                name: String::from("mid"),
                ty: ModelType::F32,
            },
            ModelStructMember {
                name: String::from("high"),
                ty: ModelType::F32,
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
            Kind::ColorPalette,
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
        assert_eq!(Kind::Amplitude.storage(), ModelType::F32);
        assert_eq!(Kind::Frequency.storage(), ModelType::F32);
    }

    #[test]
    fn int_scalar_storages() {
        assert_eq!(Kind::Count.storage(), ModelType::I32);
        assert_eq!(Kind::Choice.storage(), ModelType::I32);
    }

    #[test]
    fn position_storages() {
        assert_eq!(Kind::Position2d.storage(), ModelType::Vec2);
        assert_eq!(Kind::Position3d.storage(), ModelType::Vec3);
    }

    #[test]
    fn texture_storage_has_four_int_fields() {
        let s = Kind::Texture.storage();
        match s {
            ModelType::Struct { fields, .. } => {
                assert_eq!(fields.len(), 4);
                for m in fields {
                    assert_eq!(m.ty, ModelType::I32);
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
            ModelType::Struct { fields, .. } => {
                assert_eq!(fields.len(), AUDIO_LEVEL_BANDS);
                assert_eq!(fields[0].name.as_str(), "low");
                assert_eq!(fields[1].name.as_str(), "mid");
                assert_eq!(fields[2].name.as_str(), "high");
                for m in &fields {
                    assert_eq!(m.ty, ModelType::F32);
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
