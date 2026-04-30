//! Author-time **default** description: what to put in a slot before the
//! loader/runtime has real resources.
//!
//! Some [`Kind`][`crate::kind::Kind`]s need more than a plain [`crate::LpsValue`]
//! at author time: **opaque handles** (e.g. [`Kind::Texture`][`crate::kind::Kind::Texture`])
//! are produced from a small **recipe** (`TextureSpec`) that the loader
//! materializes into a handle-shaped value (`docs/design/lightplayer/quantity.md`
//! §7). Value-typed Kinds use [`ValueSpec::Literal`]. Defaults are serialized
//! as [`ValueSpec`], not as an already-resolved GPU handle, so save/reload
//! round-trips author intent (`quantity.md` §7 “Conventions”).
//!
//! # TOML **literal** forms (`default = …` per `quantity.md` §10)
//!
//! The JSON/serde `ValueSpecWire` form (`{ kind = "literal", value = … }`) is
//! unchanged. **TOML** load uses a [`Kind`]- and [`Shape`]-aware path so
//! `default` can be written as a plain value:
//!
//! | Kinds / shape | TOML `default` |
//! |----------------|----------------|
//! | `Amplitude`, `Ratio`, `Phase`, `Instant`, `Duration`, `Frequency`, `Angle` | any TOML number → [`LpsValue::F32`][`LpsValue`] |
//! | `Count`, `Choice` | integer → [`LpsValue::I32`][`LpsValue`] |
//! | `Bool` | bool |
//! | `Color` | CSS string `"oklch(0.7 0.15 90)"` or table `{ space = "<str>", coords = [f,f,f] }` → struct `Color` ([`LpsType`](crate::LpsType) order: `space`, `coords`) |
//! | `ColorPalette` | authoring table `{ space, count?, entries = [[f,f,f],…] }`; lpfx may materialize this as a height-one texture resource before shader binding |
//! | `Gradient` | authoring table `{ space, method, count?, stops = [{at,c},…] }`; lpfx may materialize this as a height-one texture resource before shader binding |
//! | `Position2d` / `Position3d` | 2- or 3-long array of numbers → `Vec2` / `Vec3` |
//! | `AudioLevel` | table `{ low, mid, high }` |
//! | `Texture` | string `"black"` (v0) → [`ValueSpec::Texture`] |
//! | `Shape::Array` | TOML array, length must match, elements per element [`Slot`][`crate::shape::Slot`]’s shape |
//! | `Shape::Struct` | TOML table, one key per struct field, field **declaration** order in `LpsType` / slot list |
//!
//! The inverse is `ValueSpec::to_toml_for_kind` / `ValueSpec::to_toml_for_shape` (private helpers).
//!
//! ## Serde and equality
//!
//! `LpsValueF32` in `lps-shared` does not derive `Serialize` / `PartialEq` in
//! M2; this module uses a **private** wire form for serde and hand-written
//! [`ValueSpec`]:[`PartialEq`] (see
//! `docs/plans-old/2026-04-22-lp-domain-m2-domain-skeleton/summary.md` — “ValueSpec
//! serde via private wire enum” and hand-written `PartialEq` for `ValueSpec`).

use crate::LpsValue;
use crate::kind::{Kind, MAX_GRADIENT_STOPS, MAX_PALETTE_LEN};
use crate::prop::shape::Shape;
use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use core::fmt;

/// Load-time context for **materializing** author specs: allocating handles,
/// resolving assets, and similar.
///
/// M2 ships a minimal stub; M3+ is expected to wire a real texture allocator
/// and cache (`docs/roadmaps/2026-04-22-lp-domain/m2-domain-skeleton.md` — `LoadCtx` stub, `summary.md`).
#[derive(Default)]
pub struct LoadCtx {
    /// Monotonic counter (or future allocator state) for [`TextureSpec`] materialization in tests; not the final handle policy.
    pub next_texture_handle: i32,
}

// Private serde mirror of `LpsValue` (wire shape); see module docs.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum LpsValueWire {
    I32(i32),
    U32(u32),
    F32(f32),
    Bool(bool),
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    Vec4([f32; 4]),
    IVec2([i32; 2]),
    IVec3([i32; 3]),
    IVec4([i32; 4]),
    UVec2([u32; 2]),
    UVec3([u32; 3]),
    UVec4([u32; 4]),
    BVec2([bool; 2]),
    BVec3([bool; 3]),
    BVec4([bool; 4]),
    Mat2x2([[f32; 2]; 2]),
    Mat3x3([[f32; 3]; 3]),
    Mat4x4([[f32; 4]; 4]),
    Array(Vec<LpsValueWire>),
    Struct {
        name: Option<String>,
        fields: Vec<(String, LpsValueWire)>,
    },
}

impl From<&LpsValue> for LpsValueWire {
    fn from(v: &LpsValue) -> Self {
        match v {
            LpsValue::I32(x) => LpsValueWire::I32(*x),
            LpsValue::U32(x) => LpsValueWire::U32(*x),
            LpsValue::F32(x) => LpsValueWire::F32(*x),
            LpsValue::Bool(x) => LpsValueWire::Bool(*x),
            LpsValue::Vec2(x) => LpsValueWire::Vec2(*x),
            LpsValue::Vec3(x) => LpsValueWire::Vec3(*x),
            LpsValue::Vec4(x) => LpsValueWire::Vec4(*x),
            LpsValue::IVec2(x) => LpsValueWire::IVec2(*x),
            LpsValue::IVec3(x) => LpsValueWire::IVec3(*x),
            LpsValue::IVec4(x) => LpsValueWire::IVec4(*x),
            LpsValue::UVec2(x) => LpsValueWire::UVec2(*x),
            LpsValue::UVec3(x) => LpsValueWire::UVec3(*x),
            LpsValue::UVec4(x) => LpsValueWire::UVec4(*x),
            LpsValue::BVec2(x) => LpsValueWire::BVec2(*x),
            LpsValue::BVec3(x) => LpsValueWire::BVec3(*x),
            LpsValue::BVec4(x) => LpsValueWire::BVec4(*x),
            LpsValue::Mat2x2(x) => LpsValueWire::Mat2x2(*x),
            LpsValue::Mat3x3(x) => LpsValueWire::Mat3x3(*x),
            LpsValue::Mat4x4(x) => LpsValueWire::Mat4x4(*x),
            LpsValue::Texture2D(_) => {
                panic!("Texture2D is a runtime resource; serialize a ValueSpec::Texture recipe")
            }
            LpsValue::Array(a) => LpsValueWire::Array(a.iter().map(LpsValueWire::from).collect()),
            LpsValue::Struct { name, fields } => LpsValueWire::Struct {
                name: name.clone(),
                fields: fields
                    .iter()
                    .map(|(k, v)| (k.clone(), LpsValueWire::from(v)))
                    .collect(),
            },
        }
    }
}

impl From<LpsValueWire> for LpsValue {
    fn from(w: LpsValueWire) -> Self {
        match w {
            LpsValueWire::I32(x) => LpsValue::I32(x),
            LpsValueWire::U32(x) => LpsValue::U32(x),
            LpsValueWire::F32(x) => LpsValue::F32(x),
            LpsValueWire::Bool(x) => LpsValue::Bool(x),
            LpsValueWire::Vec2(x) => LpsValue::Vec2(x),
            LpsValueWire::Vec3(x) => LpsValue::Vec3(x),
            LpsValueWire::Vec4(x) => LpsValue::Vec4(x),
            LpsValueWire::IVec2(x) => LpsValue::IVec2(x),
            LpsValueWire::IVec3(x) => LpsValue::IVec3(x),
            LpsValueWire::IVec4(x) => LpsValue::IVec4(x),
            LpsValueWire::UVec2(x) => LpsValue::UVec2(x),
            LpsValueWire::UVec3(x) => LpsValue::UVec3(x),
            LpsValueWire::UVec4(x) => LpsValue::UVec4(x),
            LpsValueWire::BVec2(x) => LpsValue::BVec2(x),
            LpsValueWire::BVec3(x) => LpsValue::BVec3(x),
            LpsValueWire::BVec4(x) => LpsValue::BVec4(x),
            LpsValueWire::Mat2x2(x) => LpsValue::Mat2x2(x),
            LpsValueWire::Mat3x3(x) => LpsValue::Mat3x3(x),
            LpsValueWire::Mat4x4(x) => LpsValue::Mat4x4(x),
            LpsValueWire::Array(items) => LpsValue::Array(
                items
                    .into_iter()
                    .map(LpsValue::from)
                    .collect::<Vec<_>>()
                    .into_boxed_slice(),
            ),
            LpsValueWire::Struct { name, fields } => LpsValue::Struct {
                name,
                fields: fields
                    .into_iter()
                    .map(|(k, v)| (k, LpsValue::from(v)))
                    .collect(),
            },
        }
    }
}

// Internally-tagged `ValueSpec` for serde/JsonSchema; public API is `ValueSpec`.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
enum ValueSpecWire {
    Literal(LpsValueWire),
    Texture(TextureSpec),
}

impl From<&ValueSpec> for ValueSpecWire {
    fn from(s: &ValueSpec) -> Self {
        match s {
            ValueSpec::Literal(v) => ValueSpecWire::Literal(LpsValueWire::from(v)),
            ValueSpec::Texture(t) => ValueSpecWire::Texture(t.clone()),
        }
    }
}

impl From<ValueSpecWire> for ValueSpec {
    fn from(w: ValueSpecWire) -> Self {
        match w {
            ValueSpecWire::Literal(v) => ValueSpec::Literal(LpsValue::from(v)),
            ValueSpecWire::Texture(t) => ValueSpec::Texture(t),
        }
    }
}

/// Either a concrete [`LpsValue`] for value-typed kinds, or a handle recipe
/// for opaque kinds (`docs/design/lightplayer/quantity.md` §7).
#[derive(Clone, Debug)]
pub enum ValueSpec {
    /// Materializes to a clone of the same value (`quantity.md` §7).
    Literal(LpsValue),
    /// [`TextureSpec`] for [`Kind::Texture`] defaults
    /// (M2: v0 has [`TextureSpec::Black`] only, `quantity.md` §7 sketch).
    Texture(TextureSpec),
}

impl serde::Serialize for ValueSpec {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        ValueSpecWire::from(self).serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for ValueSpec {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        ValueSpecWire::deserialize(deserializer).map(ValueSpec::from)
    }
}

impl PartialEq for ValueSpec {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Literal(a), Self::Literal(b)) => a.eq(b),
            (Self::Texture(a), Self::Texture(b)) => a == b,
            _ => false,
        }
    }
}

/// Recipe to build a default **texture** when author-time data is not a raw
/// handle. M2 defines only a universal 1×1 black (`quantity.md` §7).
///
/// The lpfx render MVP is expected to extend this recipe space for generated
/// image resources such as palette/gradient strips: TOML should preserve the
/// authoring recipe, while the runtime bakes width-by-one textures for shader
/// `sampler2D` uniforms.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum TextureSpec {
    /// 1×1 fully opaque black: the universal “no texture” default
    /// (`docs/design/lightplayer/quantity.md` §7).
    Black,
}

impl ValueSpec {
    /// Produces a runtime [`LpsValue`]: **identity** for [`ValueSpec::Literal`];
    /// for [`ValueSpec::Texture`], run [`TextureSpec::materialize`] and allocate/assign handles through
    /// `ctx` (`quantity.md` §7 `ValueSpec` / “Materialization is at load time”).
    pub fn materialize(&self, ctx: &mut LoadCtx) -> LpsValue {
        match self {
            Self::Literal(v) => v.clone(),
            Self::Texture(spec) => spec.materialize(ctx),
        }
    }

    /// On-disk TOML `default` for a **scalar** slot of the given [`Kind`]. See
    /// the module’s “TOML literal forms” table.
    pub(crate) fn from_toml_for_kind(
        value: &toml::Value,
        k: Kind,
    ) -> Result<ValueSpec, FromTomlError> {
        if k == Kind::Texture {
            if let toml::Value::String(s) = value {
                if s == "black" {
                    return Ok(ValueSpec::Texture(TextureSpec::Black));
                }
            }
            return Err(FromTomlError::msg(
                "texture default must be the string \"black\" in v0",
            ));
        }

        if matches!(k, Kind::Color | Kind::ColorPalette | Kind::Gradient) {
            let v = from_toml_struct_kind(value, k)?;
            return Ok(ValueSpec::Literal(v));
        }

        if k == Kind::AudioLevel {
            return Ok(ValueSpec::Literal(lps_value_audio_level(
                value
                    .as_table()
                    .ok_or_else(|| FromTomlError::msg("audio_level default must be a table"))?,
            )?));
        }

        if k == Kind::Position2d {
            return Ok(ValueSpec::Literal(vec_n_from_toml(value, 2, "position2d")?));
        }
        if k == Kind::Position3d {
            return Ok(ValueSpec::Literal(vec_n_from_toml(value, 3, "position3d")?));
        }

        let v =
            match k {
                Kind::Amplitude
                | Kind::Ratio
                | Kind::Phase
                | Kind::Instant
                | Kind::Duration
                | Kind::Frequency
                | Kind::Angle => LpsValue::F32(toml_f32(value)?),
                Kind::Count | Kind::Choice => LpsValue::I32(toml_i32(value)?),
                Kind::Bool => LpsValue::Bool(value.as_bool().ok_or_else(|| {
                    FromTomlError::msg("bool kind expects a TOML boolean `default`")
                })?),
                Kind::Texture
                | Kind::Color
                | Kind::ColorPalette
                | Kind::Gradient
                | Kind::Position2d
                | Kind::Position3d
                | Kind::AudioLevel => {
                    return Err(FromTomlError::msg("internal: kind already handled above"));
                }
            };
        Ok(ValueSpec::Literal(v))
    }

    /// On-disk TOML `default` for a value matching `shape` (compositions recurse).
    pub(crate) fn from_toml_for_shape(
        value: &toml::Value,
        shape: &Shape,
    ) -> Result<ValueSpec, FromTomlError> {
        match shape {
            Shape::Scalar { kind, .. } => Self::from_toml_for_kind(value, *kind),
            Shape::Array {
                element,
                length,
                default: _,
            } => {
                let arr = value
                    .as_array()
                    .ok_or_else(|| FromTomlError::msg("array default must be a TOML array"))?;
                if arr.len() as u32 != *length {
                    return Err(FromTomlError::msg(
                        "array default length does not match `length`",
                    ));
                }
                let mut out = Vec::with_capacity(arr.len());
                for elt in arr {
                    match Self::from_toml_for_shape(elt, &element.shape)? {
                        ValueSpec::Literal(lv) => out.push(lv),
                        ValueSpec::Texture(_) => {
                            return Err(FromTomlError::msg(
                                "array default elements must be literal",
                            ));
                        }
                    }
                }
                Ok(ValueSpec::Literal(LpsValue::Array(out.into_boxed_slice())))
            }
            Shape::Struct { fields, default: _ } => {
                let t = value
                    .as_table()
                    .ok_or_else(|| FromTomlError::msg("struct default must be a TOML table"))?;
                let mut out_fields: Vec<(String, LpsValue)> = Vec::with_capacity(fields.len());
                for (name, slot) in fields {
                    let v = t.get(name.0.as_str()).ok_or_else(|| {
                        FromTomlError(format!("struct default table missing field `{}`", name.0))
                    })?;
                    match Self::from_toml_for_shape(v, &slot.shape)? {
                        ValueSpec::Literal(lv) => out_fields.push((name.0.clone(), lv)),
                        ValueSpec::Texture(_) => {
                            return Err(FromTomlError::msg(
                                "struct default field values must be literal in v0",
                            ));
                        }
                    }
                }
                Ok(ValueSpec::Literal(LpsValue::Struct {
                    name: None,
                    fields: out_fields,
                }))
            }
        }
    }

    /// Serialize a [`ValueSpec`]'s TOML literal (inverse of [`Self::from_toml_for_kind`]).
    pub(crate) fn to_toml_for_kind(
        spec: &ValueSpec,
        k: Kind,
    ) -> Result<toml::Value, FromTomlError> {
        match (spec, k) {
            (ValueSpec::Texture(TextureSpec::Black), Kind::Texture) => {
                Ok(toml::Value::String("black".into()))
            }
            (ValueSpec::Texture(_), _) => Err(FromTomlError::msg("texture only for Kind::Texture")),
            (ValueSpec::Literal(_), Kind::Texture) => Err(FromTomlError::msg(
                "Kind::Texture default must be ValueSpec::Texture in v0",
            )),
            (ValueSpec::Literal(v), Kind::Color) => Ok(lps_color_to_toml(v)?),
            (ValueSpec::Literal(v), Kind::ColorPalette) => Ok(lps_color_palette_to_toml(v)?),
            (ValueSpec::Literal(v), Kind::Gradient) => Ok(lps_gradient_to_toml(v)?),
            (ValueSpec::Literal(v), Kind::AudioLevel) => Ok(lps_audio_level_to_toml(v)?),
            (ValueSpec::Literal(v), Kind::Position2d) => vec2_to_toml_value(v),
            (ValueSpec::Literal(v), Kind::Position3d) => vec3_to_toml_value(v),
            (ValueSpec::Literal(v), _) if k == Kind::Bool => match v {
                LpsValue::Bool(b) => Ok(toml::Value::Boolean(*b)),
                _ => Err(FromTomlError::msg(
                    "bool literal expected in ValueSpec::Literal",
                )),
            },
            (ValueSpec::Literal(v), _) if k == Kind::Count || k == Kind::Choice => match v {
                LpsValue::I32(i) => Ok(toml::Value::Integer(i64::from(*i))),
                _ => Err(FromTomlError::msg(
                    "i32 literal expected in ValueSpec::Literal",
                )),
            },
            (ValueSpec::Literal(v), _) => match v {
                LpsValue::F32(f) => Ok(toml::Value::Float(f64::from(*f))),
                _ => Err(FromTomlError::msg("f32 scalar literal expected")),
            },
        }
    }

    /// Serialize a [`ValueSpec`]'s TOML literal (inverse of [`Self::from_toml_for_shape`]).
    pub(crate) fn to_toml_for_shape(
        spec: &ValueSpec,
        shape: &Shape,
    ) -> Result<toml::Value, FromTomlError> {
        match (spec, shape) {
            (
                ValueSpec::Texture(t),
                Shape::Scalar {
                    kind: Kind::Texture,
                    ..
                },
            ) => Self::to_toml_for_kind(&ValueSpec::Texture(t.clone()), Kind::Texture),
            (ValueSpec::Literal(_), Shape::Scalar { kind, .. }) => {
                Self::to_toml_for_kind(spec, *kind)
            }
            (ValueSpec::Texture(_), _) => Err(FromTomlError::msg(
                "aggregate default must be literal in v0",
            )),
            (
                ValueSpec::Literal(v),
                Shape::Array {
                    element, length, ..
                },
            ) => {
                let a = match v {
                    LpsValue::Array(x) => x,
                    _ => {
                        return Err(FromTomlError::msg("array spec must be LpsValue::Array"));
                    }
                };
                if a.len() as u32 != *length {
                    return Err(FromTomlError::msg("array literal length mismatch"));
                }
                let mut arr = alloc::vec::Vec::with_capacity(a.len());
                for (i, elt) in a.iter().enumerate() {
                    let s = match Self::to_toml_for_shape(
                        &ValueSpec::Literal(elt.clone()),
                        &element.shape,
                    ) {
                        Ok(t) => t,
                        Err(e) => {
                            return Err(FromTomlError(format!("array element {i}: {e}")));
                        }
                    };
                    arr.push(s);
                }
                Ok(toml::Value::Array(arr))
            }
            (ValueSpec::Literal(v), Shape::Struct { fields, .. }) => {
                let tval = match v {
                    LpsValue::Struct { fields, .. } => fields,
                    _ => {
                        return Err(FromTomlError::msg("struct spec must be LpsValue::Struct"));
                    }
                };
                let mut map: toml::map::Map<String, toml::Value> = toml::map::Map::new();
                for (n, s) in fields {
                    let lv = tval
                        .iter()
                        .find(|(k, _)| k == n.0.as_str())
                        .ok_or_else(|| {
                            FromTomlError::msg("struct literal missing a field (serialization)")
                        })?
                        .1
                        .clone();
                    let tv = ValueSpec::to_toml_for_shape(&ValueSpec::Literal(lv), &s.shape)?;
                    map.insert(n.0.clone(), tv);
                }
                Ok(toml::Value::Table(map))
            }
        }
    }
}

/// Error from `ValueSpec::from_toml_for_kind` / `ValueSpec::from_toml_for_shape` and their inverses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FromTomlError(pub String);

impl FromTomlError {
    fn msg(s: &'static str) -> Self {
        FromTomlError(String::from(s))
    }
}

impl fmt::Display for FromTomlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for FromTomlError {
    fn from(s: String) -> Self {
        FromTomlError(s)
    }
}

impl core::error::Error for FromTomlError {}

// --- literal parse helpers -------------------------------------------------

fn toml_f32(v: &toml::Value) -> Result<f32, FromTomlError> {
    match v {
        toml::Value::Float(f) => Ok(*f as f32),
        toml::Value::Integer(i) => Ok(*i as f32),
        _ => Err(FromTomlError::msg(
            "expected a TOML number (float or integer)",
        )),
    }
}

fn toml_i32(v: &toml::Value) -> Result<i32, FromTomlError> {
    v.as_integer()
        .and_then(|i| i32::try_from(i).ok())
        .ok_or_else(|| FromTomlError::msg("expected a TOML integer"))
}

/// Map authoring string → `I32` tag for `Color.*.space` / gradient method / similar.
fn colorspace_id(s: &str) -> Result<i32, FromTomlError> {
    let s = s.to_lowercase();
    let id = match s.as_str() {
        "oklch" => 0,
        "oklab" => 1,
        "linear_srgb" | "linearrgb" => 2,
        "srgb" => 3,
        "hsl" => 4,
        "hsv" => 5,
        _ => {
            return Err(FromTomlError(format!("unknown color space `{s}`")));
        }
    };
    Ok(id)
}

/// Inverse of [`colorspace_id`] for TOML output (snake_case strings, `docs/design/color.md` §4).
fn colorspace_name(id: i32) -> Result<&'static str, FromTomlError> {
    match id {
        0 => Ok("oklch"),
        1 => Ok("oklab"),
        2 => Ok("linear_srgb"),
        3 => Ok("srgb"),
        4 => Ok("hsl"),
        5 => Ok("hsv"),
        _ => Err(FromTomlError::msg("unknown color space I32 id")),
    }
}

/// Function name for CSS-style `Color` TOML serialization: `name(a b c)`.
fn colorspace_css_serialize_name(id: i32) -> Result<&'static str, FromTomlError> {
    match id {
        0 => Ok("oklch"),
        1 => Ok("oklab"),
        2 => Ok("linear_srgb"),
        3 => Ok("srgb"),
        4 => Ok("hsl"),
        5 => Ok("hsv"),
        _ => Err(FromTomlError::msg("unknown color space I32 id")),
    }
}

fn split_css_arg_tokens(body: &str) -> alloc::vec::Vec<&str> {
    let mut out = alloc::vec::Vec::new();
    for part in body.split(',') {
        for tok in part.split_whitespace() {
            if !tok.is_empty() {
                out.push(tok);
            }
        }
    }
    out
}

fn parse_f32_loose(s: &str) -> Result<f32, FromTomlError> {
    s.parse::<f32>()
        .map_err(|_| FromTomlError(format!("color: invalid number `{s}`")))
}

/// CSS `100%` style token → 0.0..=1.0
fn parse_css_percent(s: &str) -> Result<f32, FromTomlError> {
    let Some(stripped) = s.strip_suffix('%') else {
        return Err(FromTomlError::msg("color: internal percent parse"));
    };
    let p = parse_f32_loose(stripped.trim())?;
    Ok(p / 100.0)
}

/// Hue: `120`, `120deg` (and optional `turn`/`rad` as multiples of 360/2π to degrees).
fn parse_css_hue(s: &str) -> Result<f32, FromTomlError> {
    let t = s.trim();
    if let Some(n) = t.strip_suffix("deg") {
        return parse_f32_loose(n.trim());
    }
    if let Some(n) = t.strip_suffix("turn") {
        return Ok(parse_f32_loose(n.trim())? * 360.0);
    }
    if let Some(n) = t.strip_suffix("rad") {
        return Ok(parse_f32_loose(n.trim())? * 180.0 / core::f32::consts::PI);
    }
    if let Some(n) = t.strip_suffix("grad") {
        return Ok(parse_f32_loose(n.trim())? * 360.0 / 400.0);
    }
    parse_f32_loose(t)
}

/// sRGB 0–1 from one `rgb()` / `r` channel: `%` is CSS semantics; otherwise >1 means 0–255.
fn parse_rgb_channel(tok: &str) -> Result<f32, FromTomlError> {
    if tok.ends_with('%') {
        return parse_css_percent(tok);
    }
    let v = parse_f32_loose(tok)?;
    if v > 1.0 {
        return Ok((v / 255.0).clamp(0.0, 1.0));
    }
    Ok(v.clamp(0.0, 1.0))
}

/// `hsl` / `hsv` S and L/V: `%` → 0–1; else plain number, or 0–100 when > 1.
fn parse_hsl_hsv_sl(tok: &str) -> Result<f32, FromTomlError> {
    if tok.ends_with('%') {
        return parse_css_percent(tok);
    }
    let v = parse_f32_loose(tok)?;
    if v > 1.0 { Ok(v / 100.0) } else { Ok(v) }
}

fn parse_hex_color(s: &str) -> Result<(i32, [f32; 3]), FromTomlError> {
    let s = s.trim();
    if !s.starts_with('#') {
        return Err(FromTomlError::msg("color: internal hex parse"));
    }
    let hex = s.strip_prefix('#').unwrap();
    let (r, g, b) = match hex.len() {
        3 => {
            let r = u8::from_str_radix(&hex[0..1].repeat(2), 16);
            let g = u8::from_str_radix(&hex[1..2].repeat(2), 16);
            let b = u8::from_str_radix(&hex[2..3].repeat(2), 16);
            (r, g, b)
        }
        6 | 8 => {
            let h = if hex.len() == 8 { &hex[..6] } else { hex };
            let r = u8::from_str_radix(&h[0..2], 16);
            let g = u8::from_str_radix(&h[2..4], 16);
            let b = u8::from_str_radix(&h[4..6], 16);
            (r, g, b)
        }
        _ => {
            return Err(FromTomlError::msg(
                "color: hex must be #rgb, #rrggbb, or #rrggbbaa",
            ));
        }
    };
    let r = r.map_err(|_| FromTomlError::msg("color: bad hex (red)"))?;
    let g = g.map_err(|_| FromTomlError::msg("color: bad hex (green)"))?;
    let b = b.map_err(|_| FromTomlError::msg("color: bad hex (blue)"))?;
    let rf = f32::from(r) / 255.0;
    let gf = f32::from(g) / 255.0;
    let bf = f32::from(b) / 255.0;
    Ok((3, [rf, gf, bf]))
}

fn color_struct_from_space_coords(space: i32, c: [f32; 3]) -> LpsValue {
    LpsValue::Struct {
        name: Some(String::from("Color")),
        fields: alloc::vec![
            (String::from("space"), LpsValue::I32(space)),
            (String::from("coords"), LpsValue::Vec3(c)),
        ],
    }
}

/// Parse a CSS-style color string to `(space_id, coords)` for `Color` literals.
fn parse_css_color_string(s: &str) -> Result<(i32, [f32; 3]), FromTomlError> {
    let s = s.trim();
    if s.is_empty() {
        return Err(FromTomlError::msg("color: empty CSS string"));
    }
    if s.starts_with('#') {
        return parse_hex_color(s);
    }
    let open = s
        .find('(')
        .ok_or_else(|| FromTomlError::msg("color: CSS function needs `(`"))?;
    let close = s
        .rfind(')')
        .ok_or_else(|| FromTomlError::msg("color: CSS function needs `)`"))?;
    if close <= open {
        return Err(FromTomlError::msg("color: invalid `()` in CSS color"));
    }
    if close != s.len() - 1 {
        return Err(FromTomlError::msg(
            "color: unexpected text after `)` in CSS color",
        ));
    }
    let fname = s[..open].trim().to_lowercase();
    let inner = s[open + 1..close].trim();
    let toks = split_css_arg_tokens(inner);
    if toks.len() != 3 {
        return Err(FromTomlError(format!(
            "color: expected 3 channel values, got {} in `{fname}(…)`",
            toks.len()
        )));
    }
    let t0 = toks[0];
    let t1 = toks[1];
    let t2 = toks[2];
    let out = match fname.as_str() {
        "oklch" | "oklab" => {
            let x = parse_f32_loose(t0)?;
            let y = parse_f32_loose(t1)?;
            let z = parse_f32_loose(t2)?;
            let sp = if fname == "oklch" { 0 } else { 1 };
            (sp, [x, y, z])
        }
        "linear_srgb" | "linearrgb" => {
            let x = parse_f32_loose(t0)?;
            let y = parse_f32_loose(t1)?;
            let z = parse_f32_loose(t2)?;
            (2, [x, y, z])
        }
        "srgb" => {
            let x = parse_rgb_channel(t0)?;
            let y = parse_rgb_channel(t1)?;
            let z = parse_rgb_channel(t2)?;
            (3, [x, y, z])
        }
        "rgb" | "rgba" => {
            let x = parse_rgb_channel(t0)?;
            let y = parse_rgb_channel(t1)?;
            let z = parse_rgb_channel(t2)?;
            (3, [x, y, z])
        }
        "hsl" | "hsla" => {
            let h = parse_css_hue(t0)?;
            let s = parse_hsl_hsv_sl(t1)?;
            let l = parse_hsl_hsv_sl(t2)?;
            (4, [h, s, l])
        }
        "hsv" | "hsva" => {
            let h = parse_css_hue(t0)?;
            let s = parse_hsl_hsv_sl(t1)?;
            let v = parse_hsl_hsv_sl(t2)?;
            (5, [h, s, v])
        }
        _ => {
            return Err(FromTomlError(format!(
                "color: unknown CSS color function `{fname}`"
            )));
        }
    };
    Ok(out)
}

/// Trim trailing zeros for a compact TOML/CSS representation.
fn fmt_css_coord(f: f32) -> String {
    let f = if f == 0.0f32 { 0.0f32 } else { f };
    let mut s = format!("{f:.6}");
    while s.contains('.') && (s.ends_with('0') || s.ends_with('.')) {
        s.pop();
    }
    s
}

fn interp_method_id(s: &str) -> Result<i32, FromTomlError> {
    let s = s.to_lowercase();
    match s.as_str() {
        "linear" => Ok(0),
        "cubic" => Ok(1),
        "step" => Ok(2),
        _ => Err(FromTomlError(format!(
            "unknown gradient interpolation method `{s}`"
        ))),
    }
}

fn interp_method_name(id: i32) -> Result<&'static str, FromTomlError> {
    match id {
        0 => Ok("linear"),
        1 => Ok("cubic"),
        2 => Ok("step"),
        _ => Err(FromTomlError::msg("unknown gradient method I32 id")),
    }
}

/// Parse `LpsValue` for struct kinds (Color, ColorPalette, Gradient).
fn from_toml_struct_kind(value: &toml::Value, k: Kind) -> Result<LpsValue, FromTomlError> {
    match k {
        Kind::Color => lps_value_color(value),
        Kind::ColorPalette => {
            let t = value
                .as_table()
                .ok_or_else(|| FromTomlError::msg("expected a TOML table"))?;
            lps_value_color_palette(t)
        }
        Kind::Gradient => {
            let t = value
                .as_table()
                .ok_or_else(|| FromTomlError::msg("expected a TOML table"))?;
            lps_value_gradient(t)
        }
        _ => Err(FromTomlError::msg("internal: not a struct color kind")),
    }
}

fn lps_value_color(v: &toml::Value) -> Result<LpsValue, FromTomlError> {
    match v {
        toml::Value::String(s) => {
            let (id, c) = parse_css_color_string(s)?;
            Ok(color_struct_from_space_coords(id, c))
        }
        toml::Value::Table(t) => lps_value_color_table(t),
        _ => Err(FromTomlError::msg(
            "color: expected a CSS string or a table { space, coords }",
        )),
    }
}

fn lps_value_color_table(
    t: &toml::map::Map<String, toml::Value>,
) -> Result<LpsValue, FromTomlError> {
    let space = t
        .get("space")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| FromTomlError::msg("color: missing `space` (string)"))?;
    let coords = t
        .get("coords")
        .ok_or_else(|| FromTomlError::msg("color: missing `coords`"))?;
    let v3 = vec3_from_toml(coords, "color.coords")?;
    Ok(LpsValue::Struct {
        name: Some(String::from("Color")),
        fields: alloc::vec![
            (String::from("space"), LpsValue::I32(colorspace_id(space)?)),
            (String::from("coords"), v3),
        ],
    })
}

fn lps_value_color_palette(
    t: &toml::map::Map<String, toml::Value>,
) -> Result<LpsValue, FromTomlError> {
    let space = t
        .get("space")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| FromTomlError::msg("color_palette: missing `space`"))?;
    let entries = t
        .get("entries")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| FromTomlError::msg("color_palette: missing `entries` array"))?;
    let count = if let Some(c) = t.get("count").and_then(toml::Value::as_integer) {
        c as u32
    } else {
        entries.len() as u32
    };
    if count as usize > MAX_PALETTE_LEN as usize {
        return Err(FromTomlError::msg(
            "color_palette: count exceeds MAX_PALETTE_LEN",
        ));
    }
    if (entries.len() as u32) < count {
        return Err(FromTomlError::msg(
            "color_palette: not enough `entries` for `count`",
        ));
    }
    let mut v3s: alloc::vec::Vec<LpsValue> = alloc::vec::Vec::new();
    for e in entries.iter().take(count as usize) {
        v3s.push(vec3_from_toml(e, "color_palette.entries")?);
    }
    while v3s.len() < MAX_PALETTE_LEN as usize {
        v3s.push(LpsValue::Vec3([0.0, 0.0, 0.0]));
    }
    let entries_lps = LpsValue::Array(
        v3s.into_iter()
            .collect::<alloc::vec::Vec<_>>()
            .into_boxed_slice(),
    );
    Ok(LpsValue::Struct {
        name: Some(String::from("ColorPalette")),
        fields: alloc::vec![
            (String::from("space"), LpsValue::I32(colorspace_id(space)?)),
            (String::from("count"), LpsValue::I32(count as i32)),
            (String::from("entries"), entries_lps),
        ],
    })
}

fn lps_value_audio_level(
    t: &toml::map::Map<String, toml::Value>,
) -> Result<LpsValue, FromTomlError> {
    let low = t
        .get("low")
        .ok_or_else(|| FromTomlError::msg("audio_level: missing `low`"))?;
    let mid = t
        .get("mid")
        .ok_or_else(|| FromTomlError::msg("audio_level: missing `mid`"))?;
    let high = t
        .get("high")
        .ok_or_else(|| FromTomlError::msg("audio_level: missing `high`"))?;
    Ok(LpsValue::Struct {
        name: Some(String::from("AudioLevel")),
        fields: alloc::vec![
            (String::from("low"), LpsValue::F32(toml_f32(low)?)),
            (String::from("mid"), LpsValue::F32(toml_f32(mid)?)),
            (String::from("high"), LpsValue::F32(toml_f32(high)?)),
        ],
    })
}

fn lps_value_gradient(t: &toml::map::Map<String, toml::Value>) -> Result<LpsValue, FromTomlError> {
    let space = t
        .get("space")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| FromTomlError::msg("gradient: missing `space`"))?;
    let method = t
        .get("method")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| FromTomlError::msg("gradient: missing `method`"))?;
    let method_id = interp_method_id(method)?;
    let stops = t
        .get("stops")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| FromTomlError::msg("gradient: missing `stops` array"))?;
    let count = if let Some(c) = t.get("count").and_then(toml::Value::as_integer) {
        c as u32
    } else {
        stops.len() as u32
    };
    if count as usize > MAX_GRADIENT_STOPS as usize {
        return Err(FromTomlError::msg(
            "gradient: count exceeds MAX_GRADIENT_STOPS",
        ));
    }
    if (stops.len() as u32) < count {
        return Err(FromTomlError::msg(
            "gradient: not enough `stops` for `count`",
        ));
    }
    let mut out: alloc::vec::Vec<LpsValue> = alloc::vec::Vec::new();
    for s in stops.iter().take(count as usize) {
        out.push(gradient_stop_from_toml(s)?);
    }
    while out.len() < MAX_GRADIENT_STOPS as usize {
        out.push(gradient_stop_default());
    }
    let stops_lps = LpsValue::Array(
        out.into_iter()
            .collect::<alloc::vec::Vec<_>>()
            .into_boxed_slice(),
    );
    Ok(LpsValue::Struct {
        name: Some(String::from("Gradient")),
        fields: alloc::vec![
            (String::from("space"), LpsValue::I32(colorspace_id(space)?)),
            (String::from("method"), LpsValue::I32(method_id)),
            (String::from("count"), LpsValue::I32(count as i32)),
            (String::from("stops"), stops_lps),
        ],
    })
}

fn gradient_stop_from_toml(v: &toml::Value) -> Result<LpsValue, FromTomlError> {
    let t = v
        .as_table()
        .ok_or_else(|| FromTomlError::msg("gradient stop must be a table"))?;
    let at = t
        .get("at")
        .ok_or_else(|| FromTomlError::msg("gradient stop: missing `at`"))?;
    let c = t
        .get("c")
        .ok_or_else(|| FromTomlError::msg("gradient stop: missing `c` (vec3)"))?;
    let cv = vec3_from_toml(c, "stop.c")?;
    Ok(LpsValue::Struct {
        name: Some(String::from("GradientStop")),
        fields: alloc::vec![
            (String::from("at"), LpsValue::F32(toml_f32(at)?)),
            (String::from("c"), cv),
        ],
    })
}

fn gradient_stop_default() -> LpsValue {
    LpsValue::Struct {
        name: Some(String::from("GradientStop")),
        fields: alloc::vec![
            (String::from("at"), LpsValue::F32(0.0)),
            (String::from("c"), LpsValue::Vec3([0.0, 0.0, 0.0])),
        ],
    }
}

fn vec3_from_toml(v: &toml::Value, _ctx: &str) -> Result<LpsValue, FromTomlError> {
    let a = v
        .as_array()
        .ok_or_else(|| FromTomlError::msg("expected a 3-long TOML array"))?;
    if a.len() != 3 {
        return Err(FromTomlError::msg("expected exactly 3 coordinates"));
    }
    let x = toml_f32(&a[0])?;
    let y = toml_f32(&a[1])?;
    let z = toml_f32(&a[2])?;
    Ok(LpsValue::Vec3([x, y, z]))
}

fn vec_n_from_toml(v: &toml::Value, n: usize, _ctx: &str) -> Result<LpsValue, FromTomlError> {
    let a = v
        .as_array()
        .ok_or_else(|| FromTomlError::msg("expected a TOML array for position default"))?;
    if a.len() != n {
        return Err(FromTomlError::msg("position default: wrong array length"));
    }
    if n == 2 {
        let x = toml_f32(&a[0])?;
        let y = toml_f32(&a[1])?;
        return Ok(LpsValue::Vec2([x, y]));
    }
    if n == 3 {
        return vec3_from_toml(v, "position3d");
    }
    Err(FromTomlError::msg("internal: bad vec_n"))
}

fn lps_color_to_toml(v: &LpsValue) -> Result<toml::Value, FromTomlError> {
    let LpsValue::Struct { name, fields } = v else {
        return Err(FromTomlError::msg(
            "Color literal must be a struct LpsValue",
        ));
    };
    if name.as_deref() != Some("Color") {
        return Err(FromTomlError::msg("Color literal: wrong struct name"));
    }
    let sp = find_field_i32(fields, "space")?;
    let co = find_field_vec3_value(fields, "coords")?;
    let css = colorspace_css_serialize_name(sp)?;
    let s = format!(
        "{}({} {} {})",
        css,
        fmt_css_coord(co[0]),
        fmt_css_coord(co[1]),
        fmt_css_coord(co[2])
    );
    Ok(toml::Value::String(s))
}

fn lps_color_palette_to_toml(v: &LpsValue) -> Result<toml::Value, FromTomlError> {
    let LpsValue::Struct { name, fields } = v else {
        return Err(FromTomlError::msg("ColorPalette must be a struct LpsValue"));
    };
    if name.as_deref() != Some("ColorPalette") {
        return Err(FromTomlError::msg("ColorPalette: wrong struct name"));
    }
    let sp = find_field_i32(fields, "space")?;
    let count = find_field_i32(fields, "count")? as u32;
    let entries = find_field_array(fields, "entries")?;
    let mut m: toml::map::Map<String, toml::Value> = toml::map::Map::new();
    m.insert(
        String::from("space"),
        toml::Value::String(colorspace_name(sp)?.to_string()),
    );
    m.insert(
        "count".to_string(),
        toml::Value::Integer(i64::from(count as i32)),
    );
    let arr = slice_to_vec3_toml(&entries[0..(count as usize).min(MAX_PALETTE_LEN as usize)])?;
    m.insert("entries".to_string(), toml::Value::Array(arr));
    Ok(toml::Value::Table(m))
}

fn lps_gradient_to_toml(v: &LpsValue) -> Result<toml::Value, FromTomlError> {
    let LpsValue::Struct { name, fields } = v else {
        return Err(FromTomlError::msg("Gradient must be a struct LpsValue"));
    };
    if name.as_deref() != Some("Gradient") {
        return Err(FromTomlError::msg("Gradient: wrong struct name"));
    }
    let sp = find_field_i32(fields, "space")?;
    let method = find_field_i32(fields, "method")?;
    let count = find_field_i32(fields, "count")? as u32;
    let stops = find_field_array(fields, "stops")?;
    let mut m: toml::map::Map<String, toml::Value> = toml::map::Map::new();
    m.insert(
        String::from("space"),
        toml::Value::String(colorspace_name(sp)?.to_string()),
    );
    m.insert(
        "method".to_string(),
        toml::Value::String(interp_method_name(method)?.to_string()),
    );
    m.insert(
        "count".to_string(),
        toml::Value::Integer(i64::from(count as i32)),
    );
    let n = (count as usize)
        .min(stops.len())
        .min(MAX_GRADIENT_STOPS as usize);
    let mut a = alloc::vec::Vec::new();
    for s in &stops[..n] {
        a.push(gradient_stop_to_toml(s)?);
    }
    m.insert("stops".to_string(), toml::Value::Array(a));
    Ok(toml::Value::Table(m))
}

fn gradient_stop_to_toml(s: &LpsValue) -> Result<toml::Value, FromTomlError> {
    let LpsValue::Struct { fields, name } = s else {
        return Err(FromTomlError::msg("stop must be struct"));
    };
    if name.as_deref() != Some("GradientStop") {
        return Err(FromTomlError::msg("stop: bad name"));
    }
    let at = find_field_f32(fields, "at")?;
    let c = find_field_vec3_value(fields, "c")?;
    let mut t: toml::map::Map<String, toml::Value> = toml::map::Map::new();
    t.insert("at".to_string(), toml::Value::Float(f64::from(at)));
    t.insert("c".to_string(), vec3_to_toml_array(&c)?);
    Ok(toml::Value::Table(t))
}

fn lps_audio_level_to_toml(v: &LpsValue) -> Result<toml::Value, FromTomlError> {
    let LpsValue::Struct { name, fields } = v else {
        return Err(FromTomlError::msg("AudioLevel must be a struct LpsValue"));
    };
    if name.as_deref() != Some("AudioLevel") {
        return Err(FromTomlError::msg("AudioLevel: wrong struct name"));
    }
    let low = find_field_f32(fields, "low")?;
    let mid = find_field_f32(fields, "mid")?;
    let high = find_field_f32(fields, "high")?;
    let mut m: toml::map::Map<String, toml::Value> = toml::map::Map::new();
    m.insert("low".to_string(), toml::Value::Float(f64::from(low)));
    m.insert("mid".to_string(), toml::Value::Float(f64::from(mid)));
    m.insert("high".to_string(), toml::Value::Float(f64::from(high)));
    Ok(toml::Value::Table(m))
}

fn find_field_f32(fields: &[(String, LpsValue)], key: &str) -> Result<f32, FromTomlError> {
    let v = fields
        .iter()
        .find(|(k, _)| k == key)
        .ok_or_else(|| FromTomlError::msg("missing f32 field"))?
        .1
        .clone();
    match v {
        LpsValue::F32(f) => Ok(f),
        _ => Err(FromTomlError::msg("not F32")),
    }
}

fn find_field_i32(fields: &[(String, LpsValue)], key: &str) -> Result<i32, FromTomlError> {
    let v = fields
        .iter()
        .find(|(k, _)| k == key)
        .ok_or_else(|| FromTomlError::msg("missing I32 field"))?
        .1
        .clone();
    match v {
        LpsValue::I32(i) => Ok(i),
        _ => Err(FromTomlError::msg("not I32")),
    }
}

fn find_field_vec3(fields: &[(String, LpsValue)], key: &str) -> Result<LpsValue, FromTomlError> {
    let v = fields
        .iter()
        .find(|(k, _)| k == key)
        .ok_or_else(|| FromTomlError::msg("missing field"))?
        .1
        .clone();
    if matches!(&v, LpsValue::Vec3(_)) {
        return Ok(v);
    }
    Err(FromTomlError::msg("not Vec3"))
}

fn find_field_vec3_value(
    fields: &[(String, LpsValue)],
    key: &str,
) -> Result<[f32; 3], FromTomlError> {
    let v = find_field_vec3(fields, key)?;
    match v {
        LpsValue::Vec3(a) => Ok(a),
        _ => Err(FromTomlError::msg("not Vec3")),
    }
}

fn find_field_array(
    fields: &[(String, LpsValue)],
    key: &str,
) -> Result<alloc::vec::Vec<LpsValue>, FromTomlError> {
    let v = fields
        .iter()
        .find(|(k, _)| k == key)
        .ok_or_else(|| FromTomlError::msg("missing array field"))?
        .1
        .clone();
    match v {
        LpsValue::Array(b) => Ok(b.iter().cloned().collect()),
        _ => Err(FromTomlError::msg("not array")),
    }
}

fn slice_to_vec3_toml(s: &[LpsValue]) -> Result<alloc::vec::Vec<toml::Value>, FromTomlError> {
    let mut out = alloc::vec::Vec::with_capacity(s.len());
    for e in s {
        let LpsValue::Vec3(a) = e else {
            return Err(FromTomlError::msg("entry not Vec3"));
        };
        out.push(vec3_to_toml_array(a)?);
    }
    Ok(out)
}

fn vec2_to_toml_value(v: &LpsValue) -> Result<toml::Value, FromTomlError> {
    let LpsValue::Vec2(a) = v else {
        return Err(FromTomlError::msg(
            "position2d literal must be Vec2 LpsValue",
        ));
    };
    Ok(toml::Value::Array(alloc::vec![
        toml::Value::Float(f64::from(a[0])),
        toml::Value::Float(f64::from(a[1])),
    ]))
}

fn vec3_to_toml_value(v: &LpsValue) -> Result<toml::Value, FromTomlError> {
    let LpsValue::Vec3(a) = v else {
        return Err(FromTomlError::msg(
            "position3d literal must be Vec3 LpsValue",
        ));
    };
    vec3_to_toml_array(a)
}

fn vec3_to_toml_array(a: &[f32; 3]) -> Result<toml::Value, FromTomlError> {
    Ok(toml::Value::Array(alloc::vec![
        toml::Value::Float(f64::from(a[0])),
        toml::Value::Float(f64::from(a[1])),
        toml::Value::Float(f64::from(a[2])),
    ]))
}

impl TextureSpec {
    /// Returns the handle-shaped `LpsValue` struct for [`Kind::Texture`]
    /// storage (`quantity.md` §3, texture struct).
    pub fn materialize(&self, ctx: &mut LoadCtx) -> LpsValue {
        match self {
            Self::Black => texture_handle_value(ctx, 0, 1, 1),
        }
    }
}

/// Delegates to the private `ValueSpecWire` type’s `JsonSchema` impl so recursive [`Shape`] / [`Slot`](crate::prop::shape::Slot)
/// can derive schemas without exposing the wire type.
#[cfg(feature = "schema-gen")]
impl schemars::JsonSchema for ValueSpec {
    fn schema_name() -> alloc::borrow::Cow<'static, str> {
        <ValueSpecWire as schemars::JsonSchema>::schema_name()
    }

    fn schema_id() -> alloc::borrow::Cow<'static, str> {
        <ValueSpecWire as schemars::JsonSchema>::schema_id()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        <ValueSpecWire as schemars::JsonSchema>::json_schema(generator)
    }
}

fn texture_handle_value(ctx: &mut LoadCtx, format: i32, width: i32, height: i32) -> LpsValue {
    let handle = ctx.next_texture_handle;
    LpsValue::Struct {
        name: None,
        fields: alloc::vec![
            (String::from("format"), LpsValue::I32(format)),
            (String::from("width"), LpsValue::I32(width)),
            (String::from("height"), LpsValue::I32(height)),
            (String::from("handle"), LpsValue::I32(handle)),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NodeName;
    use crate::kind::Kind;
    use crate::prop::shape::{Shape, Slot};
    use alloc::boxed::Box;

    #[test]
    fn literal_materializes_to_itself() {
        let mut ctx = LoadCtx::default();
        let spec = ValueSpec::Literal(LpsValue::F32(0.5));
        match spec.materialize(&mut ctx) {
            LpsValue::F32(v) => assert_eq!(v, 0.5),
            other => panic!("expected F32(0.5), got {other:?}"),
        }
    }

    #[test]
    fn texture_black_materializes_to_handle_zero() {
        let mut ctx = LoadCtx::default();
        let spec = ValueSpec::Texture(TextureSpec::Black);
        let v = spec.materialize(&mut ctx);
        match v {
            LpsValue::Struct { fields, .. } => {
                let handle = fields
                    .iter()
                    .find(|(n, _)| n == "handle")
                    .expect("handle field");
                match &handle.1 {
                    LpsValue::I32(h) => assert_eq!(*h, 0),
                    _ => panic!("handle must be I32"),
                }
            }
            other => panic!("expected Struct, got {other:?}"),
        }
    }

    #[test]
    fn f32_literal_round_trips_in_toml_for_amplitude() {
        let v = toml::Value::Float(1.0);
        let s = ValueSpec::from_toml_for_kind(&v, Kind::Amplitude).unwrap();
        let t = ValueSpec::to_toml_for_kind(&s, Kind::Amplitude).unwrap();
        assert!(matches!(&t, toml::Value::Float(f) if (*f as f32 - 1.0).abs() < 1e-6));
    }

    #[test]
    fn i32_literal_round_trips_in_toml_for_count() {
        let v = toml::Value::Integer(4);
        let s = ValueSpec::from_toml_for_kind(&v, Kind::Count).unwrap();
        let t = ValueSpec::to_toml_for_kind(&s, Kind::Count).unwrap();
        assert_eq!(t.as_integer(), Some(4));
    }

    #[test]
    fn bool_literal_round_trips_in_toml_for_bool() {
        let v = toml::Value::Boolean(true);
        let s = ValueSpec::from_toml_for_kind(&v, Kind::Bool).unwrap();
        let t = ValueSpec::to_toml_for_kind(&s, Kind::Bool).unwrap();
        assert_eq!(t.as_bool(), Some(true));
    }

    #[test]
    fn color_literal_round_trips_in_toml() {
        let css = toml::Value::String("oklch(0.7 0.15 90)".into());
        let s = ValueSpec::from_toml_for_kind(&css, Kind::Color).unwrap();
        let out = ValueSpec::to_toml_for_kind(&s, Kind::Color).unwrap();
        assert_eq!(
            out.as_str(),
            Some("oklch(0.7 0.15 90)"),
            "serialized Color must be a CSS function string: {out:?}"
        );
    }

    #[test]
    fn color_literal_accepts_table_backward_compat() {
        let toml = r#"
        space = "oklch"
        coords = [0.72, 0.14, 285]
        "#;
        let table: toml::Table = toml::from_str(toml).unwrap();
        let v = toml::Value::Table(table);
        let s = ValueSpec::from_toml_for_kind(&v, Kind::Color).unwrap();
        let out = ValueSpec::to_toml_for_kind(&s, Kind::Color).unwrap();
        assert_eq!(out.as_str(), Some("oklch(0.72 0.14 285)"));
    }

    #[test]
    fn color_literal_parses_oklch_hex_and_rgb_strings() {
        let g = 87.0f32 / 255.0;
        let b = 51.0f32 / 255.0;
        let cases: [(&str, toml::Value, i32, f32, f32, f32); 3] = [
            (
                "oklch",
                toml::Value::String("oklch(0.72 0.14 285)".into()),
                0,
                0.72,
                0.14,
                285.0,
            ),
            ("hex", toml::Value::String("#ff5733".into()), 3, 1.0, g, b),
            (
                "rgb",
                toml::Value::String("rgb(255 87 51)".into()),
                3,
                1.0,
                g,
                b,
            ),
        ];
        for (_label, tval, sp, a, b0, c0) in cases {
            let s = ValueSpec::from_toml_for_kind(&tval, Kind::Color).unwrap();
            let got = s.materialize(&mut LoadCtx::default());
            let LpsValue::Struct { fields, .. } = got else {
                panic!("struct color");
            };
            let space = fields
                .iter()
                .find_map(|(n, v)| (n == "space").then(|| v))
                .expect("space");
            let coords = fields
                .iter()
                .find_map(|(n, v)| (n == "coords").then(|| v))
                .expect("coords");
            let LpsValue::I32(sid) = space else {
                panic!("space");
            };
            let LpsValue::Vec3([x, y, z]) = coords else {
                panic!("coords");
            };
            assert_eq!(*sid, sp);
            assert!((x - a).abs() < 0.001, "x {x} vs {a}");
            assert!((y - b0).abs() < 0.001, "y {y} vs {b0}");
            assert!((z - c0).abs() < 0.001, "z {z} vs {c0}");
        }
    }

    #[test]
    fn audio_level_literal_round_trips_in_toml() {
        let toml = r#"
        low  = 0.1
        mid  = 0.2
        high = 0.3
        "#;
        let table: toml::Table = toml::from_str(toml).unwrap();
        let v = toml::Value::Table(table);
        let s = ValueSpec::from_toml_for_kind(&v, Kind::AudioLevel).unwrap();
        let out = ValueSpec::to_toml_for_kind(&s, Kind::AudioLevel).unwrap();
        let t = out.as_table().expect("table");
        assert!(t.get("low").is_some());
    }

    #[test]
    fn position2d_array_literal_round_trips() {
        let v = toml::Value::Array(alloc::vec![
            toml::Value::Float(0.0),
            toml::Value::Float(1.0)
        ]);
        let s = ValueSpec::from_toml_for_kind(&v, Kind::Position2d).unwrap();
        let t = ValueSpec::to_toml_for_kind(&s, Kind::Position2d).unwrap();
        assert_eq!(t.as_array().map(|a| a.len()), Some(2));
    }

    #[test]
    fn texture_black_string_round_trips() {
        let v = toml::Value::String("black".into());
        let s = ValueSpec::from_toml_for_kind(&v, Kind::Texture).unwrap();
        let t = ValueSpec::to_toml_for_kind(&s, Kind::Texture).unwrap();
        assert_eq!(t.as_str(), Some("black"));
    }

    fn amp_slot() -> Shape {
        Shape::Scalar {
            kind: Kind::Amplitude,
            constraint: crate::kind::Kind::Amplitude.default_constraint(),
            default: ValueSpec::Literal(LpsValue::F32(0.0)),
        }
    }

    #[test]
    fn array_of_amplitude_literals_round_trips() {
        let v = toml::Value::Array(alloc::vec![
            toml::Value::Float(0.1),
            toml::Value::Float(0.2),
        ]);
        let shape = Shape::Array {
            element: Box::new(Slot {
                shape: amp_slot(),
                label: None,
                description: None,
                bind: None,
                present: None,
            }),
            length: 2,
            default: None,
        };
        let s = ValueSpec::from_toml_for_shape(&v, &shape).unwrap();
        let t = ValueSpec::to_toml_for_shape(&s, &shape).unwrap();
        assert_eq!(t.as_array().map(|a| a.len()), Some(2));
    }

    #[test]
    fn struct_of_two_amplitudes_round_trips() {
        let toml = r#"
        a = 0.1
        b = 0.2
        "#;
        let table: toml::Table = toml::from_str(toml).unwrap();
        let v = toml::Value::Table(table);
        let fields = alloc::vec![
            (
                NodeName::parse("a").unwrap(),
                Slot {
                    shape: amp_slot(),
                    label: None,
                    description: None,
                    bind: None,
                    present: None,
                },
            ),
            (
                NodeName::parse("b").unwrap(),
                Slot {
                    shape: amp_slot(),
                    label: None,
                    description: None,
                    bind: None,
                    present: None,
                },
            ),
        ];
        let shape = Shape::Struct {
            fields: fields.clone(),
            default: None,
        };
        let s = ValueSpec::from_toml_for_shape(&v, &shape).unwrap();
        let t = ValueSpec::to_toml_for_shape(&s, &shape).unwrap();
        assert_eq!(t.as_table().map(|m| m.len()), Some(2));
    }
}
