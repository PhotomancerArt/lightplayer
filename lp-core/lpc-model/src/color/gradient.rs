//! The [`Gradient`] value: stops in one authoring space, read one way.
//!
//! Storage is the ratified fixed-shape recipe from `docs/design/color.md` §5
//! — `{ space: I32, method: I32, count: I32, stops: Array(GradientStop, 24) }`
//! — while the serde surface is the friendly authored form (snake-case enum
//! strings, `stops` exactly as long as authored). See the [module
//! docs](super) for why the two differ.
//!
//! A discrete swatch list is a [`Gradient`] with [`InterpMethod::Step`]; there
//! is no separate palette type (D2 of the palette spike, which also raised the
//! stop bound to 24 so WLED's 18-stop gradients import whole).

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;
use serde::{Deserialize, Serialize};

use crate::{
    FromLpValue, LpType, LpValue, SlotMeta, SlotShape, SlotShapeId, SlotValue, SlotValueShape,
    StaticLpType, StaticModelStructMember, StaticSlotMeta, StaticSlotShape,
    StaticSlotShapeDescriptor, StaticSlotValueShape, StaticValueEditorHint, ToLpValue,
    ValueEditorHint, ValueRootError,
};

/// Native shape name for [`Colorspace`].
pub const COLORSPACE_SHAPE_NAME: &str = "lp::color::Colorspace";

/// Native shape name for [`InterpMethod`].
pub const INTERP_METHOD_SHAPE_NAME: &str = "lp::color::InterpMethod";

/// Native shape name for [`Gradient`].
pub const GRADIENT_SHAPE_NAME: &str = "lp::color::Gradient";

/// Stops in a [`Gradient`]'s fixed `stops` array — the storage size, not the
/// authored size.
///
/// 24 rather than `color.md`'s original 16 (D2 of the palette spike): WLED
/// gradients carry up to 18 stops and importing them must not truncate.
/// Growing this is a one-constant change that widens every stored gradient;
/// authored values above it are a load error, never silent truncation.
pub const MAX_GRADIENT_STOPS: u32 = 24;

/// Stops below which a gradient does not describe a ramp.
pub const MIN_GRADIENT_STOPS: u32 = 2;

/// Space the stop colors are authored in, and the space interpolation happens
/// in (`docs/design/color.md` §4, §6).
///
/// The `repr(i32)` values are serialized into [`LpValue`] storage: **stable,
/// never renumbered**, new spaces append.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(i32)]
pub enum Colorspace {
    /// Linear-light sRGB primaries — the canonical numeric space; a no-op at
    /// binding time.
    #[default]
    LinearSrgb = 0,
    /// Display-encoded sRGB (gamma ~2.2). What hex colors and most pickers
    /// emit.
    Srgb = 1,
    /// Hue / saturation / lightness, a cylindrical reparametrization of sRGB.
    Hsl = 2,
    /// Hue / saturation / value (HSB). The "rainbow" space.
    Hsv = 3,
    /// Perceptually uniform Cartesian — the best space for color math.
    Oklab = 4,
    /// Perceptually uniform cylindrical (polar Oklab) — the best space for
    /// hue manipulation.
    Oklch = 5,
}

impl Colorspace {
    /// Snake-case wire tag.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LinearSrgb => "linear_srgb",
            Self::Srgb => "srgb",
            Self::Hsl => "hsl",
            Self::Hsv => "hsv",
            Self::Oklab => "oklab",
            Self::Oklch => "oklch",
        }
    }

    /// Parse a snake-case wire tag.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "linear_srgb" => Some(Self::LinearSrgb),
            "srgb" => Some(Self::Srgb),
            "hsl" => Some(Self::Hsl),
            "hsv" => Some(Self::Hsv),
            "oklab" => Some(Self::Oklab),
            "oklch" => Some(Self::Oklch),
            _ => None,
        }
    }

    /// Every space, in declaration order (pickers, tests).
    #[must_use]
    pub const fn all() -> &'static [Colorspace] {
        &[
            Colorspace::LinearSrgb,
            Colorspace::Srgb,
            Colorspace::Hsl,
            Colorspace::Hsv,
            Colorspace::Oklab,
            Colorspace::Oklch,
        ]
    }

    /// Stable integer tag written into the `space: I32` storage field.
    #[must_use]
    pub const fn as_i32(self) -> i32 {
        self as i32
    }

    /// Inverse of [`Colorspace::as_i32`]; `None` for an unknown tag rather
    /// than a guess.
    #[must_use]
    pub const fn from_i32(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::LinearSrgb),
            1 => Some(Self::Srgb),
            2 => Some(Self::Hsl),
            3 => Some(Self::Hsv),
            4 => Some(Self::Oklab),
            5 => Some(Self::Oklch),
            _ => None,
        }
    }
}

/// How a sample between two stops is taken (`docs/design/color.md` §6).
///
/// Interpolation happens in the gradient's own [`Colorspace`], never in
/// canonical — that is the point of authoring in Oklch.
///
/// The `repr(i32)` values are serialized into [`LpValue`] storage: **stable,
/// never renumbered**.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(i32)]
pub enum InterpMethod {
    /// No interpolation; a sample picks the nearest stop at or before `t`.
    /// This is what a discrete swatch palette is.
    Step = 0,
    /// Linear interpolation between adjacent stops.
    #[default]
    Linear = 1,
    /// Smoothstep / cubic interpolation.
    Smooth = 2,
}

impl InterpMethod {
    /// Snake-case wire tag.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Step => "step",
            Self::Linear => "linear",
            Self::Smooth => "smooth",
        }
    }

    /// Parse a snake-case wire tag.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "step" => Some(Self::Step),
            "linear" => Some(Self::Linear),
            "smooth" => Some(Self::Smooth),
            _ => None,
        }
    }

    /// Every method, in declaration order (pickers, tests).
    #[must_use]
    pub const fn all() -> &'static [InterpMethod] {
        &[
            InterpMethod::Step,
            InterpMethod::Linear,
            InterpMethod::Smooth,
        ]
    }

    /// Stable integer tag written into the `method: I32` storage field.
    #[must_use]
    pub const fn as_i32(self) -> i32 {
        self as i32
    }

    /// Inverse of [`InterpMethod::as_i32`]; `None` for an unknown tag.
    #[must_use]
    pub const fn from_i32(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::Step),
            1 => Some(Self::Linear),
            2 => Some(Self::Smooth),
            _ => None,
        }
    }
}

// --- Colorspace / InterpMethod: hand-rolled string leaves.
//
// Same shape as `Waveform` (`time/phasor_config.rs`), factored into a local
// macro because there are two of them; the `impl_string_leaf!` precedent in
// `nodes/shader/shader_slot_def.rs` is private to its own module. Note the
// leaf `LpType` is `String` while these same enums are `I32` *inside* the
// gradient struct — the two-surface split described in the module docs.

macro_rules! impl_color_string_leaf {
    ($ty:ty, $shape_name:ident, $expected:literal) => {
        impl fmt::Display for $ty {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl Serialize for $ty {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> Deserialize<'de> for $ty {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::parse(&value).ok_or_else(|| {
                    serde::de::Error::custom(alloc::format!(
                        concat!("unknown ", $expected, " {:?}"),
                        value
                    ))
                })
            }
        }

        impl ToLpValue for $ty {
            fn to_lp_value(&self) -> LpValue {
                LpValue::String(self.as_str().to_string())
            }
        }

        impl FromLpValue for $ty {
            fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
                match value {
                    LpValue::String(value) => Self::parse(value)
                        .ok_or_else(|| ValueRootError::new(concat!("expected ", $expected))),
                    other => Err(ValueRootError::new(alloc::format!(
                        "expected String, got {other:?}"
                    ))),
                }
            }
        }

        impl SlotValue for $ty {
            const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name($shape_name);
            const STATIC_VALUE_SHAPE_DESCRIPTOR: Option<StaticSlotValueShape> = Some(
                StaticSlotValueShape::new(<$ty as SlotValue>::SHAPE_ID, StaticLpType::String),
            );

            fn value_shape() -> SlotValueShape {
                SlotValueShape {
                    id: <$ty as SlotValue>::SHAPE_ID,
                    ty: LpType::String,
                    meta: SlotMeta::empty(),
                    editor: ValueEditorHint::Plain,
                }
            }
        }

        impl StaticSlotShape for $ty {
            const SHAPE_ID: SlotShapeId = <Self as SlotValue>::SHAPE_ID;
            const STATIC_SLOT_SHAPE_DESCRIPTOR: Option<&'static StaticSlotShapeDescriptor> =
                Some(&StaticSlotShapeDescriptor::Value {
                    shape: StaticSlotValueShape::new(
                        <$ty as SlotValue>::SHAPE_ID,
                        StaticLpType::String,
                    ),
                });

            fn slot_shape() -> SlotShape {
                SlotShape::leaf(<Self as SlotValue>::value_shape())
            }

            fn shape_name() -> Option<&'static str> {
                Some($shape_name)
            }
        }
    };
}

impl_color_string_leaf!(Colorspace, COLORSPACE_SHAPE_NAME, "colorspace");
impl_color_string_leaf!(InterpMethod, INTERP_METHOD_SHAPE_NAME, "interp method");

/// One gradient stop: a position and a color in the gradient's space.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GradientStop {
    /// Position along the ramp, `[0, 1]`.
    pub at: f32,
    /// Color coordinates in the gradient's [`Colorspace`]. Deliberately
    /// unconstrained beyond `[0, 1]` per `color.md` §10 rule 6 — overshoot is
    /// meaningful.
    pub c: [f32; 3],
}

/// A palette: stops in one authoring space, read one way.
///
/// The stop list is authored-length here; [`ToLpValue`] pads it out to
/// [`MAX_GRADIENT_STOPS`] with zeroed stops and records the authored length
/// in `count`, which is what shaders iterate.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Gradient {
    /// Space the stops are authored in and interpolated in.
    pub space: Colorspace,
    /// How samples between stops are taken.
    pub method: InterpMethod,
    /// Authored stops, [`MIN_GRADIENT_STOPS`]..=[`MAX_GRADIENT_STOPS`] once
    /// [`Gradient::validate`] passes.
    pub stops: Vec<GradientStop>,
}

impl Gradient {
    /// Check the invariants storage depends on.
    ///
    /// Enforced: stop count in `2..=24`, and every `at` finite and within
    /// `[0, 1]`. Authored *order* is deliberately not enforced — finite `at`
    /// values are enough to guarantee the list is sortable, and consumers
    /// sort by `at` when they resolve. Stop colors are unchecked (`color.md`
    /// §10 rule 6: out-of-gamut and boosted coordinates are legal).
    pub fn validate(&self) -> Result<(), GradientError> {
        let count = self.stops.len();
        if count < MIN_GRADIENT_STOPS as usize {
            return Err(GradientError::TooFewStops(count));
        }
        if count > MAX_GRADIENT_STOPS as usize {
            return Err(GradientError::TooManyStops(count));
        }
        for (index, stop) in self.stops.iter().enumerate() {
            if !stop.at.is_finite() || !(0.0..=1.0).contains(&stop.at) {
                return Err(GradientError::StopPosition(index));
            }
        }
        Ok(())
    }
}

impl Default for Gradient {
    /// The slot default nobody authored: a linear black→white sRGB ramp.
    fn default() -> Self {
        Self {
            space: Colorspace::Srgb,
            method: InterpMethod::Linear,
            stops: Vec::from([
                GradientStop {
                    at: 0.0,
                    c: [0.0, 0.0, 0.0],
                },
                GradientStop {
                    at: 1.0,
                    c: [1.0, 1.0, 1.0],
                },
            ]),
        }
    }
}

/// Why a [`Gradient`] or [`crate::GradientConfig`] is not storable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GradientError {
    /// Fewer than [`MIN_GRADIENT_STOPS`] stops.
    TooFewStops(usize),
    /// More than [`MAX_GRADIENT_STOPS`] stops — a load error, never a
    /// truncation (`color.md` §5).
    TooManyStops(usize),
    /// The stop at this index has a non-finite or out-of-`[0, 1]` position.
    StopPosition(usize),
    /// Fewer than [`crate::MIN_CYCLE_SET`] gradients in a cycle set.
    TooFewCycleEntries(usize),
    /// More than [`crate::MAX_CYCLE_SET`] gradients in a cycle set.
    TooManyCycleEntries(usize),
    /// The cycle entry at this index is itself invalid.
    CycleEntry(usize),
}

impl fmt::Display for GradientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooFewStops(count) => {
                write!(
                    f,
                    "gradient needs at least {MIN_GRADIENT_STOPS} stops, got {count}"
                )
            }
            Self::TooManyStops(count) => {
                write!(
                    f,
                    "gradient allows at most {MAX_GRADIENT_STOPS} stops, got {count}"
                )
            }
            Self::StopPosition(index) => {
                write!(
                    f,
                    "gradient stop {index} position must be finite and within [0, 1]"
                )
            }
            Self::TooFewCycleEntries(count) => write!(
                f,
                "gradient cycle needs at least {} gradients, got {count}",
                crate::MIN_CYCLE_SET
            ),
            Self::TooManyCycleEntries(count) => write!(
                f,
                "gradient cycle allows at most {} gradients, got {count}",
                crate::MAX_CYCLE_SET
            ),
            Self::CycleEntry(index) => write!(f, "gradient cycle entry {index} is invalid"),
        }
    }
}

impl core::error::Error for GradientError {}

// --- Gradient: hand-rolled fixed-shape record.
//
// Positional field reads mirror `PhasorConfig` (`time/phasor_config.rs`); the
// array padding and `count` are the `color.md` §5 recipe.

impl ToLpValue for GradientStop {
    fn to_lp_value(&self) -> LpValue {
        LpValue::Struct {
            name: Some("GradientStop".to_string()),
            fields: Vec::from([
                ("at".to_string(), self.at.to_lp_value()),
                ("c".to_string(), self.c.to_lp_value()),
            ]),
        }
    }
}

impl FromLpValue for GradientStop {
    fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
        let LpValue::Struct { name, fields } = value else {
            return Err(ValueRootError::new("expected GradientStop struct"));
        };
        if name.as_deref() != Some("GradientStop") || fields.len() != 2 {
            return Err(ValueRootError::new("expected GradientStop struct"));
        }
        Ok(Self {
            at: read_field(fields, 0, "GradientStop", "at")?,
            c: read_field(fields, 1, "GradientStop", "c")?,
        })
    }
}

impl ToLpValue for Gradient {
    fn to_lp_value(&self) -> LpValue {
        let mut stops = Vec::with_capacity(MAX_GRADIENT_STOPS as usize);
        stops.extend(
            self.stops
                .iter()
                .take(MAX_GRADIENT_STOPS as usize)
                .map(ToLpValue::to_lp_value),
        );
        // Padding is zeroed and never read: `count` bounds every consumer.
        let padding = GradientStop::default().to_lp_value();
        stops.resize(MAX_GRADIENT_STOPS as usize, padding);

        LpValue::Struct {
            name: Some("Gradient".to_string()),
            fields: Vec::from([
                ("space".to_string(), self.space.as_i32().to_lp_value()),
                ("method".to_string(), self.method.as_i32().to_lp_value()),
                (
                    "count".to_string(),
                    stop_count_tag(self.stops.len()).to_lp_value(),
                ),
                ("stops".to_string(), LpValue::Array(stops)),
            ]),
        }
    }
}

impl FromLpValue for Gradient {
    fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
        let LpValue::Struct { name, fields } = value else {
            return Err(ValueRootError::new("expected Gradient struct"));
        };
        if name.as_deref() != Some("Gradient") || fields.len() != 4 {
            return Err(ValueRootError::new("expected Gradient struct"));
        }

        let space = colorspace_from_tag(read_field(fields, 0, "Gradient", "space")?)?;
        let method = interp_method_from_tag(read_field(fields, 1, "Gradient", "method")?)?;
        let count = stop_count_from_tag(read_field(fields, 2, "Gradient", "count")?)?;
        let stops = read_stop_array(fields, count)?;

        Ok(Self {
            space,
            method,
            stops,
        })
    }
}

/// Positional field read shared by every hand-rolled struct in this module.
///
/// `owner` names the struct so a mismatch says which shape disagreed, the way
/// `PhasorConfig`'s single-purpose version does.
pub(crate) fn read_field<T: FromLpValue>(
    fields: &[(String, LpValue)],
    index: usize,
    owner: &str,
    name: &str,
) -> Result<T, ValueRootError> {
    match fields.get(index) {
        Some((field_name, value)) if field_name == name => T::from_lp_value(value),
        _ => Err(ValueRootError::new(alloc::format!(
            "expected {owner}.{name}"
        ))),
    }
}

fn colorspace_from_tag(tag: i32) -> Result<Colorspace, ValueRootError> {
    Colorspace::from_i32(tag)
        .ok_or_else(|| ValueRootError::new(alloc::format!("unknown Gradient.space tag {tag}")))
}

fn interp_method_from_tag(tag: i32) -> Result<InterpMethod, ValueRootError> {
    InterpMethod::from_i32(tag)
        .ok_or_else(|| ValueRootError::new(alloc::format!("unknown Gradient.method tag {tag}")))
}

/// Authored stop count as its `I32` storage tag, saturating at the bound so a
/// too-long list writes a readable value; [`Gradient::validate`] is what
/// rejects it.
fn stop_count_tag(count: usize) -> i32 {
    count.min(MAX_GRADIENT_STOPS as usize) as i32
}

fn stop_count_from_tag(tag: i32) -> Result<usize, ValueRootError> {
    let count = usize::try_from(tag).unwrap_or(usize::MAX);
    if !(MIN_GRADIENT_STOPS as usize..=MAX_GRADIENT_STOPS as usize).contains(&count) {
        return Err(ValueRootError::new(alloc::format!(
            "Gradient.count must be {MIN_GRADIENT_STOPS}..={MAX_GRADIENT_STOPS}, got {tag}"
        )));
    }
    Ok(count)
}

/// Read the fixed `stops` array and keep only the `count` authored entries.
fn read_stop_array(
    fields: &[(String, LpValue)],
    count: usize,
) -> Result<Vec<GradientStop>, ValueRootError> {
    let Some(("stops", LpValue::Array(stops))) =
        fields.get(3).map(|(name, value)| (name.as_str(), value))
    else {
        return Err(ValueRootError::new("expected Gradient.stops"));
    };
    if stops.len() != MAX_GRADIENT_STOPS as usize {
        return Err(ValueRootError::new(alloc::format!(
            "Gradient.stops must hold {MAX_GRADIENT_STOPS} entries, got {}",
            stops.len()
        )));
    }
    stops
        .iter()
        .take(count)
        .map(GradientStop::from_lp_value)
        .collect()
}

const GRADIENT_STOP_STATIC_TYPE: StaticLpType = StaticLpType::Struct {
    name: Some("GradientStop"),
    fields: &[
        StaticModelStructMember {
            name: "at",
            ty: StaticLpType::F32,
        },
        StaticModelStructMember {
            name: "c",
            ty: StaticLpType::Vec3,
        },
    ],
};

pub(crate) const GRADIENT_STATIC_TYPE: StaticLpType = StaticLpType::Struct {
    name: Some("Gradient"),
    fields: &[
        StaticModelStructMember {
            name: "space",
            ty: StaticLpType::I32,
        },
        StaticModelStructMember {
            name: "method",
            ty: StaticLpType::I32,
        },
        StaticModelStructMember {
            name: "count",
            ty: StaticLpType::I32,
        },
        StaticModelStructMember {
            name: "stops",
            ty: StaticLpType::Array(&GRADIENT_STOP_STATIC_TYPE, MAX_GRADIENT_STOPS as usize),
        },
    ],
};

/// The canonical [`Gradient`] storage recipe (`docs/design/color.md` §5).
///
/// [`crate::Kind::Gradient`]'s legacy `storage()` recipe delegates here so
/// there is exactly one definition of the shape.
#[must_use]
pub fn gradient_lp_type() -> LpType {
    GRADIENT_STATIC_TYPE.to_owned_type()
}

impl SlotValue for Gradient {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name(GRADIENT_SHAPE_NAME);
    const STATIC_VALUE_SHAPE_DESCRIPTOR: Option<StaticSlotValueShape> =
        Some(StaticSlotValueShape {
            id: <Gradient as SlotValue>::SHAPE_ID,
            ty: GRADIENT_STATIC_TYPE,
            meta: StaticSlotMeta::EMPTY,
            editor: StaticValueEditorHint::Plain,
        });

    fn value_shape() -> SlotValueShape {
        SlotValueShape {
            id: <Gradient as SlotValue>::SHAPE_ID,
            ty: gradient_lp_type(),
            meta: SlotMeta::empty(),
            editor: ValueEditorHint::Plain,
        }
    }
}

impl StaticSlotShape for Gradient {
    const SHAPE_ID: SlotShapeId = <Self as SlotValue>::SHAPE_ID;
    const STATIC_SLOT_SHAPE_DESCRIPTOR: Option<&'static StaticSlotShapeDescriptor> =
        Some(&StaticSlotShapeDescriptor::Value {
            shape: StaticSlotValueShape {
                id: <Gradient as SlotValue>::SHAPE_ID,
                ty: GRADIENT_STATIC_TYPE,
                meta: StaticSlotMeta::EMPTY,
                editor: StaticValueEditorHint::Plain,
            },
        });

    fn slot_shape() -> SlotShape {
        SlotShape::leaf(<Self as SlotValue>::value_shape())
    }

    fn shape_name() -> Option<&'static str> {
        Some(GRADIENT_SHAPE_NAME)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ModelStructMember;

    fn ramp(stops: usize) -> Gradient {
        Gradient {
            space: Colorspace::Oklch,
            method: InterpMethod::Smooth,
            stops: (0..stops)
                .map(|index| GradientStop {
                    at: index as f32 / (stops.max(2) - 1) as f32,
                    c: [index as f32, 0.5, 0.25],
                })
                .collect(),
        }
    }

    #[test]
    fn colorspace_tags_round_trip() {
        for &space in Colorspace::all() {
            assert_eq!(Colorspace::parse(space.as_str()), Some(space));
            assert_eq!(Colorspace::from_i32(space.as_i32()), Some(space));
            assert_eq!(Colorspace::from_lp_value(&space.to_lp_value()), Ok(space));
        }
        assert_eq!(Colorspace::parse("rgb"), None);
        assert_eq!(Colorspace::from_i32(6), None);
        assert_eq!(Colorspace::default(), Colorspace::LinearSrgb);
    }

    /// `color.md` §4/§6 pin these integers; renumbering silently rewrites
    /// every stored gradient.
    #[test]
    fn enum_integer_tags_are_the_ratified_ones() {
        assert_eq!(Colorspace::LinearSrgb.as_i32(), 0);
        assert_eq!(Colorspace::Srgb.as_i32(), 1);
        assert_eq!(Colorspace::Hsl.as_i32(), 2);
        assert_eq!(Colorspace::Hsv.as_i32(), 3);
        assert_eq!(Colorspace::Oklab.as_i32(), 4);
        assert_eq!(Colorspace::Oklch.as_i32(), 5);

        assert_eq!(InterpMethod::Step.as_i32(), 0);
        assert_eq!(InterpMethod::Linear.as_i32(), 1);
        assert_eq!(InterpMethod::Smooth.as_i32(), 2);
    }

    #[test]
    fn interp_method_tags_round_trip() {
        for &method in InterpMethod::all() {
            assert_eq!(InterpMethod::parse(method.as_str()), Some(method));
            assert_eq!(InterpMethod::from_i32(method.as_i32()), Some(method));
            assert_eq!(
                InterpMethod::from_lp_value(&method.to_lp_value()),
                Ok(method)
            );
        }
        assert_eq!(InterpMethod::parse("cubic"), None);
        assert_eq!(InterpMethod::default(), InterpMethod::Linear);
    }

    #[test]
    fn color_enums_serde_use_snake_case_strings() {
        assert_eq!(
            serde_json::to_string(&Colorspace::LinearSrgb).unwrap(),
            "\"linear_srgb\""
        );
        assert_eq!(
            serde_json::from_str::<Colorspace>("\"oklch\"").unwrap(),
            Colorspace::Oklch
        );
        assert!(serde_json::from_str::<Colorspace>("\"rgb\"").is_err());

        assert_eq!(
            serde_json::to_string(&InterpMethod::Smooth).unwrap(),
            "\"smooth\""
        );
        assert!(serde_json::from_str::<InterpMethod>("\"cubic\"").is_err());
    }

    #[test]
    fn default_gradient_is_a_two_stop_black_to_white_ramp() {
        let gradient = Gradient::default();

        assert_eq!(gradient.space, Colorspace::Srgb);
        assert_eq!(gradient.method, InterpMethod::Linear);
        assert_eq!(gradient.stops.len(), 2);
        assert_eq!(gradient.stops[0].c, [0.0, 0.0, 0.0]);
        assert_eq!(gradient.stops[1].c, [1.0, 1.0, 1.0]);
        assert_eq!(gradient.validate(), Ok(()));
    }

    #[test]
    fn validate_enforces_the_stop_bounds() {
        assert_eq!(ramp(1).validate(), Err(GradientError::TooFewStops(1)));
        assert_eq!(
            ramp(MAX_GRADIENT_STOPS as usize).validate(),
            Ok(()),
            "24 stops is the bound, not one past it"
        );
        assert_eq!(
            ramp(MAX_GRADIENT_STOPS as usize + 1).validate(),
            Err(GradientError::TooManyStops(25))
        );
    }

    #[test]
    fn validate_rejects_a_non_finite_or_out_of_range_stop_position() {
        let mut gradient = Gradient::default();
        gradient.stops[1].at = f32::NAN;
        assert_eq!(gradient.validate(), Err(GradientError::StopPosition(1)));

        gradient.stops[1].at = 1.5;
        assert_eq!(gradient.validate(), Err(GradientError::StopPosition(1)));
    }

    #[test]
    fn gradient_round_trips_through_lp_value() {
        let gradient = ramp(5);

        assert_eq!(
            Gradient::from_lp_value(&gradient.to_lp_value()).unwrap(),
            gradient
        );
        assert_eq!(
            Gradient::from_lp_value(&Gradient::default().to_lp_value()).unwrap(),
            Gradient::default()
        );
    }

    /// Storage is the fixed `color.md` §5 recipe: always 24 stops, authored
    /// length in `count`, integer enum tags.
    #[test]
    fn gradient_storage_is_the_fixed_recipe() {
        let LpValue::Struct { name, fields } = ramp(3).to_lp_value() else {
            panic!("Gradient storage must be a Struct");
        };

        assert_eq!(name.as_deref(), Some("Gradient"));
        assert_eq!(fields[0], ("space".to_string(), LpValue::I32(5)));
        assert_eq!(fields[1], ("method".to_string(), LpValue::I32(2)));
        assert_eq!(fields[2], ("count".to_string(), LpValue::I32(3)));

        let LpValue::Array(stops) = &fields[3].1 else {
            panic!("stops must be an Array");
        };
        assert_eq!(stops.len(), MAX_GRADIENT_STOPS as usize);
        assert_eq!(stops[23], GradientStop::default().to_lp_value());
    }

    #[test]
    fn gradient_rejects_foreign_and_out_of_bounds_storage() {
        assert!(Gradient::from_lp_value(&LpValue::F32(1.0)).is_err());
        assert!(
            Gradient::from_lp_value(&LpValue::Struct {
                name: Some("Other".to_string()),
                fields: Vec::new(),
            })
            .is_err()
        );

        let bad_count = |count: i32| {
            let LpValue::Struct { name, mut fields } = Gradient::default().to_lp_value() else {
                unreachable!()
            };
            fields[2].1 = LpValue::I32(count);
            LpValue::Struct { name, fields }
        };
        assert!(Gradient::from_lp_value(&bad_count(1)).is_err());
        assert!(Gradient::from_lp_value(&bad_count(25)).is_err());
        assert!(Gradient::from_lp_value(&bad_count(-1)).is_err());
    }

    #[test]
    fn gradient_serde_is_the_friendly_authored_form() {
        let json = serde_json::to_string(&Gradient::default()).unwrap();

        assert!(json.contains("\"space\":\"srgb\""), "{json}");
        assert!(json.contains("\"method\":\"linear\""), "{json}");
        assert!(json.contains("\"at\":0.0"), "{json}");

        let parsed: Gradient = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, Gradient::default());
    }

    #[test]
    fn static_and_dynamic_gradient_shapes_agree() {
        let dynamic = <Gradient as SlotValue>::value_shape();
        let static_shape =
            <Gradient as SlotValue>::STATIC_VALUE_SHAPE_DESCRIPTOR.expect("static descriptor");

        assert_eq!(static_shape.to_owned_value_shape(), dynamic);
        assert_eq!(
            dynamic.id,
            SlotShapeId::from_static_name(GRADIENT_SHAPE_NAME)
        );

        for (dynamic, static_shape) in [
            (
                <Colorspace as SlotValue>::value_shape(),
                <Colorspace as SlotValue>::STATIC_VALUE_SHAPE_DESCRIPTOR.expect("static"),
            ),
            (
                <InterpMethod as SlotValue>::value_shape(),
                <InterpMethod as SlotValue>::STATIC_VALUE_SHAPE_DESCRIPTOR.expect("static"),
            ),
        ] {
            assert_eq!(static_shape.to_owned_value_shape(), dynamic);
        }
    }

    /// The legacy `Kind::Gradient` storage recipe and this module must be the
    /// same shape — that is the whole point of the delegation.
    #[test]
    fn kind_gradient_storage_delegates_here() {
        assert_eq!(crate::Kind::Gradient.storage(), gradient_lp_type());

        let LpType::Struct { fields, .. } = gradient_lp_type() else {
            panic!("Gradient type must be a Struct");
        };
        let stop = LpType::Struct {
            name: Some("GradientStop".to_string()),
            fields: Vec::from([
                ModelStructMember {
                    name: "at".to_string(),
                    ty: LpType::F32,
                },
                ModelStructMember {
                    name: "c".to_string(),
                    ty: LpType::Vec3,
                },
            ]),
        };
        assert_eq!(
            fields[3].ty,
            LpType::Array(alloc::boxed::Box::new(stop), MAX_GRADIENT_STOPS as usize)
        );
    }
}
